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

camera_frame_queues = []

main_frame_source = ""
small_frame_sources = []

if 'CONFIGURATION_NAME' in os.environ:
    configuration_name = os.environ['CONFIGURATION_NAME']
    short_env_var_prefix = (configuration_name + '-').upper().replace('-', '_')
    # For every k8s service, an env var is set in every node with its ports.  The
    # format is <SERVICE_NAME_IN_CAPS>_SERVICE_PORT_<PORT_NAME>.  Here, we will query
    # these values on the streaming app node, to get the exposed port number ... but
    # we will assume that the ports are named 'grpc', if the name changes, THIS CODE
    # WILL BREAK.
    env_var_prefix = short_env_var_prefix + 'SVC_SERVICE_'
    grpc_port = os.environ[env_var_prefix + 'PORT_GRPC'] # instance services are using the same port by default
    main_frame_source = "{0}:{1}".format(os.environ[env_var_prefix + 'HOST'], grpc_port)
    instance_service_hosts = filter(
        lambda name: name.startswith(short_env_var_prefix) and not name.startswith(env_var_prefix) and name.endswith('_SERVICE_HOST'),
        os.environ)
    camera_count = 0
    for svc_host_env_var in instance_service_hosts:
        url = "{0}:{1}".format(os.environ[svc_host_env_var], grpc_port)
        small_frame_sources.append(url)
        camera_count += 1

else:
    camera_count = int(os.environ['CAMERA_COUNT'])
    main_frame_source = "{0}:80".format(os.environ['CAMERAS_SOURCE_SVC'])
    for camera_id in range(1, camera_count + 1):
        url = "{0}:80".format(os.environ['CAMERA{0}_SOURCE_SVC'.format(camera_id)])
        small_frame_sources.append(url)

for camera_id in range(camera_count + 1):
    camera_frame_queues.append(queue.Queue(1))

app = Flask(__name__)

@app.route('/')
# Home page for video streaming.
def index():
    return render_template('index.html', camera_count = camera_count)

# Generator function for video streaming.
def gen(frame_queue, verbose=False):
    while True:
        frame = frame_queue.get(True, None)
        if (verbose):
            logging.info("Sending frame %d" % len(frame))
        yield (b'--frame\r\n'
               b'Content-Type: image/jpeg\r\n\r\n' + frame + b'\r\n')

# Gets response and puts it in frame queue.
def response_wrapper(frame_queue):
    return Response(gen(frame_queue),
                    mimetype='multipart/x-mixed-replace; boundary=frame')

@app.route('/camera_frame_feed/<camera_id>')
# Gets frame feed for specified camera.
def camera_frame_feed(camera_id=0):
    camera_id = int(camera_id)
    if (camera_id <= camera_count):
        return response_wrapper(camera_frame_queues[camera_id])
    return None

def run_webserver():
    app.run(host='0.0.0.0', threaded=True)

# Loops, creating gRPC client and grabing frame from camera serving specified url.
def get_frames(url, frame_queue):
    logging.info("Starting get_frames(%s)" % url)
    while True:
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

print("Starting...", flush=True)

format = "%(asctime)s: %(message)s"
logging.basicConfig(format=format, level=logging.INFO, datefmt="%H:%M:%S")

webserver_thread = threading.Thread(target=run_webserver)
webserver_thread.start()

cameras_frame_thread = threading.Thread(target=get_frames, args=(main_frame_source, camera_frame_queues[0]))
cameras_frame_thread.start()
camera_frame_threads = [cameras_frame_thread]

for camera_id in range(1, camera_count + 1):
    camera_frame_thread = threading.Thread(target=get_frames, args=(small_frame_sources[camera_id - 1], camera_frame_queues[camera_id]))
    camera_frame_thread.start()
    camera_frame_threads.append(camera_frame_thread)

print("Started", flush=True)
webserver_thread.join()
for camera_frame_thread in camera_frame_threads:
    camera_frame_thread.join()
print("Done", flush=True)
