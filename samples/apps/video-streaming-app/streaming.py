import sys
import os

from time import sleep

import grpc

import camera_pb2
import camera_pb2_grpc

import threading
import logging
from concurrent import futures
import queue
import traceback

from flask import Flask, render_template, Response

from kubernetes import client, config
import re

camera_frame_queues = []
small_frame_sources = []

def get_camera_list(configuration_name):
    camera_list = []
    config.load_incluster_config()
    coreV1Api = client.CoreV1Api()
    ret = coreV1Api.list_service_for_all_namespaces(watch=False)
    p = re.compile(configuration_name + "-[\da-f]{6}-svc")
    for svc in ret.items:
        if not p.match(svc.metadata.name):
            continue
        grpc_ports = list(filter(lambda port: port.name == "grpc", svc.spec.ports))
        if (len(grpc_ports) == 1):
            url = "{0}:{1}".format(svc.spec.cluster_ip, grpc_ports[0].port)
            camera_list.append(url)
    camera_list.sort()
    return camera_list

app = Flask(__name__)

@app.route('/')
# Home page for video streaming.
def index():
    global camera_frame_queues
    return render_template('index.html', camera_count=len(camera_frame_queues)-1)
    
@app.route('/camera_list')
# Returns the current list of cameras to allow for refresh
def camera_list():
    global small_frame_sources
    logging.info(small_frame_sources)
    return ",".join(small_frame_sources)

# Generator function for video streaming.
def gen(frame_queue, verbose=False):
    while True:
        frame = frame_queue.get(True, None)
        yield (b'--frame\r\n'
               b'Content-Type: image/jpeg\r\n\r\n' + frame + b'\r\n')

# Gets response and puts it in frame queue.
def response_wrapper(frame_queue):
    return Response(gen(frame_queue),
                    mimetype='multipart/x-mixed-replace; boundary=frame')

@app.route('/camera_frame_feed/<camera_id>')
# Gets frame feed for specified camera.
def camera_frame_feed(camera_id=0):
    global camera_frame_queues
    camera_id = int(camera_id)
    if (camera_id <= len(camera_frame_queues)):
        logging.info("camera_feed %d" % camera_id)
        return response_wrapper(camera_frame_queues[camera_id])
    return None

# Updates set of cameras based on set of camera instance services
def refresh_cameras(camera_frame_threads, small_frame_sources, camera_frame_queues, stop_event):
    while True:
        sleep(1)
        camera_list = get_camera_list(os.environ['CONFIGURATION_NAME'])
        if camera_list != small_frame_sources:
            old_count = len(small_frame_sources)
            new_count = len(camera_list)
            logging.info("Camera change detected, old: %d, new: %d" % (old_count, new_count))
            if old_count != new_count:
                if old_count < new_count:
                    for x in range(new_count - old_count):
                        camera_frame_queues.append(queue.Queue(1))
                    small_frame_sources[:] = camera_list
                else:
                    small_frame_sources[:] = camera_list
                    camera_frame_queues[:] = camera_frame_queues[:(old_count - new_count)]
            else:
                small_frame_sources[:] = camera_list
            logging.info(small_frame_sources)
            schedule_get_frames(
                camera_frame_threads, small_frame_sources, camera_frame_queues, stop_event)

def run_webserver():
    app.run(host='0.0.0.0', threaded=True)

# Loops, creating gRPC client and grabing frame from camera serving specified url.
def get_frames(url, frame_queue, stop_event):
    logging.info("Starting get_frames(%s)" % url)
    while not stop_event.wait(0.01):
        try:
            client_channel = grpc.insecure_channel(url, options=(
                ('grpc.use_local_subchannel_pool', 1),))
            camera_stub = camera_pb2_grpc.CameraStub(client_channel)
            frame = camera_stub.GetFrame(camera_pb2.NotifyRequest())
            frame = frame.frame
            client_channel.close()

            frame_received = False
            # prevent stale data
            if (len(frame) > 0):
                if (frame_queue.full()):
                    try:
                        frame_queue.get(False)
                    except:
                        pass
                frame_queue.put(frame, False)
                frame_received = True
            
            if (frame_received):
                sleep(1)

        except:
            logging.info("[%s] Exception %s" % (url, traceback.format_exc()))
            sleep(1)

# schedules frame polling threads
def schedule_get_frames(camera_frame_threads, small_frame_sources, camera_frame_queues, stop_event):
    if camera_frame_threads:
        stop_event.set()
        for camera_frame_thread in camera_frame_threads:
            camera_frame_thread.join()
        stop_event.clear()
        camera_frame_threads.clear()

    cameras_frame_thread = threading.Thread(target=get_frames, args=(main_frame_source, camera_frame_queues[0], stop_event))
    cameras_frame_thread.start()
    camera_frame_threads.append(cameras_frame_thread)

    for camera_id in range(1, len(small_frame_sources) + 1):
        camera_frame_thread = threading.Thread(target=get_frames, args=(small_frame_sources[camera_id - 1], camera_frame_queues[camera_id], stop_event))
        camera_frame_thread.start()
        camera_frame_threads.append(camera_frame_thread)

print("Starting...", flush=True)
logging.basicConfig(format="%(asctime)s: %(message)s", level=logging.INFO, datefmt="%H:%M:%S")

main_frame_source = ""

if 'CONFIGURATION_NAME' in os.environ:
    # Expecting source service ports to be named grpc

    configuration_name = os.environ['CONFIGURATION_NAME']

    config.load_incluster_config()
    coreV1Api = client.CoreV1Api()
    ret = coreV1Api.list_service_for_all_namespaces(watch=False)
    for svc in ret.items:
        if svc.metadata.name == configuration_name + "-svc":
            grpc_ports = list(
                filter(lambda port: port.name == "grpc", svc.spec.ports))
            if (len(grpc_ports) == 1):
                main_frame_source = "{0}:{1}".format(
                    svc.spec.cluster_ip, grpc_ports[0].port)

    small_frame_sources = get_camera_list(configuration_name)
    camera_count = len(small_frame_sources)
else:
    camera_count = int(os.environ['CAMERA_COUNT'])
    main_frame_source = "{0}:80".format(os.environ['CAMERAS_SOURCE_SVC'])
    for camera_id in range(1, camera_count + 1):
        url = "{0}:80".format(
            os.environ['CAMERA{0}_SOURCE_SVC'.format(camera_id)])
        small_frame_sources.append(url)

for camera_id in range(camera_count + 1):
    camera_frame_queues.append(queue.Queue(1))

webserver_thread = threading.Thread(target=run_webserver)
webserver_thread.start()

stop_event = threading.Event()
camera_frame_threads = []
schedule_get_frames(camera_frame_threads, small_frame_sources, camera_frame_queues, stop_event)

if 'CONFIGURATION_NAME' in os.environ:
    refresh_thread = threading.Thread(target=refresh_cameras, args=(camera_frame_threads, small_frame_sources, camera_frame_queues, stop_event))
    refresh_thread.start()

print("Started", flush=True)
webserver_thread.join()
print("Done", flush=True)
