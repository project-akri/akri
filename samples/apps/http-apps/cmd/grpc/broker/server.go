package main

import (
	"context"
	"fmt"
	"io/ioutil"
	"log"
	"net/http"

	pb "github.com/deislabs/akri/http-extensibility/protos"
)

var _ pb.DeviceServiceServer = (*Server)(nil)

// Server is a type that implements pb.DeviceServiceServer
type Server struct {
	DeviceURL string
}

// NewServer is a function that returns a new Server
func NewServer(deviceURL string) *Server {
	return &Server{
		DeviceURL: deviceURL,
	}
}

// ReadSensor is a method that implements the pb.HTTPServer interface
func (s *Server) ReadSensor(ctx context.Context, rqst *pb.ReadSensorRequest) (*pb.ReadSensorResponse, error) {
	log.Println("[read_sensor] Entered")
	resp, err := http.Get(s.DeviceURL)
	if err != nil {
		return &pb.ReadSensorResponse{}, err
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode > 299 {
		log.Printf("[read_sensor] Response status: %d", resp.StatusCode)
		return &pb.ReadSensorResponse{}, fmt.Errorf("response code: %d", resp.StatusCode)
	}

	body, err := ioutil.ReadAll(resp.Body)
	if err != nil {
		return &pb.ReadSensorResponse{}, err
	}

	log.Printf("[read_sensor] Response body: %s", body)
	return &pb.ReadSensorResponse{
		Value: string(body),
	}, nil
}
