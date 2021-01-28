# Flask app that acts as a sample application for the OPC UA Monitoring Brokers.
# It periodically gets values from the brokers via grpc, detects whether they are anomaly values by comparing
# them to the training data set (data.csv) using Local Outlier Factor, and displays a log of the values on a
# web server, showing anomalies in red text.

from datetime import datetime
from flask import Flask, render_template, make_response
from numpy import genfromtxt
from sklearn.neighbors import LocalOutlierFactor
from time import sleep
import grpc
import logging
import numpy as np
import opcua_node_pb2
import opcua_node_pb2_grpc
import os
import threading
import traceback


app = Flask(__name__)

# Log of all values reported by OPC UA servers and the time they were reported
values_log = []

# Interval for getting new values
fetch_interval_seconds = 1

@app.route('/')
# Home page for anomaly detection results
def index():
    return render_template('index.html')

# Endpoint for grabbing current log of values
@app.route('/values_log')
def values_log_state():
    global values_log
    return ";".join(values_log)

# Gets the url of the service the OPC UA monitoring brokers are serving values on
def get_grpc_url():
    if 'CONFIGURATION_NAME' in os.environ:
        configuration_name = os.environ['CONFIGURATION_NAME']
        short_env_var_prefix = (configuration_name +
                                '-').upper().replace('-', '_')
        # For every k8s service, an env var is set in every node with its ports.  The
        # format is <SERVICE_NAME_IN_CAPS>_SERVICE_PORT_<PORT_NAME>.  Here, we will query
        # these values on the streaming app node, to get the exposed port number ... but
        # we will assume that the ports are named 'grpc', if the name changes, THIS CODE
        # WILL BREAK.
        env_var_prefix = short_env_var_prefix + 'SVC_SERVICE_'
        # instance services are using the same port by default
        grpc_port = os.environ[env_var_prefix + 'PORT_GRPC']
        opcua_brokers_service = "{0}:{1}".format(
            os.environ[env_var_prefix + 'HOST'], grpc_port)
        return opcua_brokers_service
    else:
        raise Exception(
            "CONFIGURATION_NAME not loaded as environment variable")

# Creates log entry that reports the opc ua server, value, current time, and whether the value is an anomaly
def make_log_entry(server, value, is_anomaly):
    anomaly_character = "Y" if is_anomaly else "N"
    return "{0}, {1}, {2}{3}".format(server, value, datetime.now(), anomaly_character)

# Periodically gets the latest value from the OPC UA monitoring brokers' grpc servers
def continuously_get_values():
    global values_log
    url = get_grpc_url()
    data = get_data_from_csv()
    logging.info("Starting to call GetValue on endpoint %s", url)
    while True:
        try:
            channel = grpc.insecure_channel(url, options=(
                ('grpc.use_local_subchannel_pool', 1),))
            stub = opcua_node_pb2_grpc.OpcuaNodeStub(channel)
            value_response = stub.GetValue(opcua_node_pb2.ValueRequest())
            channel.close()
            if test_new_value(data, value_response.value) == -1:
                values_log.append(make_log_entry(
                    value_response.opcua_server, value_response.value, True))
                logging.info("Latest anomaly added to log is {0}".format(
                    values_log[len(values_log) - 1]))
            else:
                # Check if server previously had anomaly and remove it if back to normal
                values_log.append(make_log_entry(
                    value_response.opcua_server, value_response.value, False))
                logging.info("Latest normal value added to log is {0}".format(
                    values_log[len(values_log) - 1]))
            sleep(fetch_interval_seconds)
        except:
            logging.info("[%s] Exception %s" % (url, traceback.format_exc()))
            sleep(fetch_interval_seconds)

# Uses Local Outlier Factor to determine whether the new value is an outlier to the dataset
# Returns -1 if the value is an outlier and 1 if it is a conforming value
def test_new_value(data, new_value):
    extended_data = np.append(data, new_value)
    reshaped_data = np.reshape(extended_data, (-1, 1))
    outlier_prediction_list = LocalOutlierFactor(n_neighbors=2,
                                                 contamination=0.1,
                                                 novelty=True).fit(reshaped_data).predict()
    return outlier_prediction_list[len(outlier_prediction_list) - 1]

# Get training data from csv
def get_data_from_csv():
    data = genfromtxt('data.csv', delimiter=',')
    print("data is", data)
    return data

# Run webserver
def run_webserver():
    app.run(host='0.0.0.0', threaded=True)

if __name__ == "__main__":
    # Set up logging
    format = "%(asctime)s: %(message)s"
    logging.basicConfig(format=format, level=logging.INFO, datefmt="%H:%M:%S")
    logging.info("Starting web server thread)")
    webserver_thread = threading.Thread(target=run_webserver)
    webserver_thread.setDaemon(True)
    webserver_thread.start()
    logging.info("Starting anomaly list updating thread")
    continuously_get_values()
