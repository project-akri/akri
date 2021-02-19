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

class CameraFeed:
    def __init__(self, url):
        global global_stop_event

        self.url = url
        self.queue = queue.Queue(1)
        self.thread = None
        self.stop_event = global_stop_event

    def __eq__(self, other):
        if other is None:
            return False
        return self.url == other.url

    def start_handler(self):
        self.thread = threading.Thread(target=self.get_frames)
        self.thread.start()

    def wait_handler(self):
        if self.thread is not None:
            self.thread.join()

    # Generator function for video streaming.
    def generator_func(self):
        while not self.stop_event.wait(0.01):
            frame = self.queue.get(True, None)
            yield (b'--frame\r\nContent-Type: image/jpeg\r\n\r\n' + frame + b'\r\n')

    # Loops, creating gRPC client and grabing frame from camera serving specified url.
    def get_frames(self):
        logging.info("Starting get_frames(%s)" % self.url)
        while not self.stop_event.wait(0.01):
            try:
                client_channel = grpc.insecure_channel(self.url, options=(
                    ('grpc.use_local_subchannel_pool', 1),))
                camera_stub = camera_pb2_grpc.CameraStub(client_channel)
                frame = camera_stub.GetFrame(camera_pb2.NotifyRequest())
                frame = frame.frame
                client_channel.close()

                frame_received = False
                # prevent stale data
                if (len(frame) > 0):
                    if (self.queue.full()):
                        try:
                            self.queue.get(False)
                        except:
                            pass
                    self.queue.put(frame, False)
                    frame_received = True
                
                if (frame_received):
                    sleep(1)

            except:
                logging.info("[%s] Exception %s" % (self.url, traceback.format_exc()))
                sleep(1)

class CameraDisplay:
    def __init__(self):
        self.main_camera = None
        self.small_cameras = []
        self.mutex = threading.Lock()

    def __eq__(self, other):
        return self.main_camera == other.main_camera and self.small_cameras == other.small_cameras
        
    def start_handlers(self):
        if self.main_camera is not None:
            self.main_camera.start_handler()
        for small_camera in self.small_cameras:
            small_camera.start_handler()

    def wait_handlers(self):
        global global_stop_event

        global_stop_event.set()
        if self.main_camera is not None:
            self.main_camera.wait_handler()
        for small_camera in self.small_cameras:
            small_camera.wait_handler()
        global_stop_event.clear()
        
    def merge(self, other):
        self.mutex.acquire()
        try:
            self.wait_handlers()

            self.main_camera = other.main_camera
            self.small_cameras = other.small_cameras

            self.start_handlers()
        finally:
            self.mutex.release()

    def count(self):
        self.mutex.acquire()    
        result = len(self.small_cameras)
        if self.main_camera is not None:
            result += 1
        self.mutex.release()
        return result

    def hash_code(self):
        self.mutex.acquire()
        cameras = ",".join([camera.url for camera in self.small_cameras])
        if self.main_camera is not None:
            cameras = "{0}+{1}".format(self.main_camera.url, cameras)
        self.mutex.release()
        return cameras

    def stream_frames(self, camera_id):
        selected_camera = None
        camera_id = int(camera_id)

        self.mutex.acquire()
        if camera_id == 0:
            selected_camera = self.main_camera
        elif camera_id - 1 < len(self.small_cameras):
            selected_camera = self.small_cameras[camera_id - 1]
        self.mutex.release()
        
        if selected_camera is None:
            return Response(None, 500)
        else:
            return Response(selected_camera.generator_func(), mimetype='multipart/x-mixed-replace; boundary=frame')

def get_camera_display(configuration_name):
    camera_display = CameraDisplay()
    
    config.load_incluster_config()
    coreV1Api = client.CoreV1Api()

    # TODO use labels instead once available
    instance_service_name_regex = re.compile(
        configuration_name + "-[\da-f]{6}-svc")

    ret = coreV1Api.list_service_for_all_namespaces(watch=False)
    for svc in ret.items:
        if svc.metadata.name == configuration_name + "-svc":
            grpc_ports = list(
                filter(lambda port: port.name == "grpc", svc.spec.ports))
            if (len(grpc_ports) == 1):
                url = "{0}:{1}".format(svc.spec.cluster_ip, grpc_ports[0].port)
                camera_display.main_camera = CameraFeed(url)
        elif instance_service_name_regex.match(svc.metadata.name):
            grpc_ports = list(
                filter(lambda port: port.name == "grpc", svc.spec.ports))
            if (len(grpc_ports) == 1):
                url = "{0}:{1}".format(svc.spec.cluster_ip, grpc_ports[0].port)
                camera_display.small_cameras.append(CameraFeed(url))

    camera_display.small_cameras.sort(key=lambda camera: camera.url)

    return camera_display

def run_webserver():
    app.run(host='0.0.0.0', threaded=True)

def refresh_cameras():
    global global_camera_display
    while True:
        sleep(1)
        camera_display = get_camera_display(os.environ['CONFIGURATION_NAME'])
        if camera_display != global_camera_display:
            global_camera_display.merge(camera_display)

global_stop_event = threading.Event()
global_camera_display = CameraDisplay()

app = Flask(__name__)

# Home page for video streaming.
@app.route('/')
def index():
    global global_camera_display
    return render_template('index.html', camera_count=global_camera_display.count(), camera_list=global_camera_display.hash_code())

# Returns the current list of cameras to allow for refresh
@app.route('/camera_list')
def camera_list():
    global global_camera_display
    logging.info("Expected cameras: %s" % global_camera_display.hash_code())
    return global_camera_display.hash_code()

# Gets frame feed for specified camera.
@app.route('/camera_frame_feed/<camera_id>')
def camera_frame_feed(camera_id=0):
    global global_camera_display
    return global_camera_display.stream_frames(camera_id)

print("Starting...", flush=True)
logging.basicConfig(format="%(asctime)s: %(message)s", level=logging.INFO, datefmt="%H:%M:%S")

if 'CONFIGURATION_NAME' in os.environ:
    # Expecting source service ports to be named grpc

    configuration_name = os.environ['CONFIGURATION_NAME']
    camera_display = get_camera_display(configuration_name)
    global_camera_display.merge(camera_display)

    refresh_thread = threading.Thread(target=refresh_cameras)
    refresh_thread.start()
else:
    camera_count = int(os.environ['CAMERA_COUNT'])
    main_camera_url = "{0}:80".format(os.environ['CAMERAS_SOURCE_SVC'])
    global_camera_display.main_camera = CameraFeed(main_camera_url)
    for camera_id in range(1, camera_count + 1):
        url = "{0}:80".format(
            os.environ['CAMERA{0}_SOURCE_SVC'.format(camera_id)])
        global_camera_display.small_cameras.append(CameraFeed(url))
    global_camera_display.start_handlers()

webserver_thread = threading.Thread(target=run_webserver)
webserver_thread.start()

print("Started", flush=True)
webserver_thread.join()
print("Done", flush=True)
