package main

import (
	"flag"
	"log"
	"net"
	"os"

	pb "github.com/deislabs/akri/http-extensibility/prots"

	"google.golang.org/grpc"
)

const (
	deviceEndpoint = "AKRI_HTTP_DEVICE_ENDPOINT"
)

var (
	grpcEndpoint = flag.String("grpc_endpoint", "", "The endpoint of this gRPC server.")
)

func main() {
	log.Println("[main] Starting gRPC server")

	flag.Parse()
	if *grpcEndpoint == "" {
		log.Fatal("[main] Unable to start server. Requires gRPC endpoint.")
	}

	deviceURL := os.Getenv(deviceEndpoint)
	if deviceURL == "" {
		log.Fatalf("Unable to determine Device URL using environment: %s", deviceEndpoint)
	}

	serverOpts := []grpc.ServerOption{}
	grpcServer := grpc.NewServer(serverOpts...)

	pb.RegisterDeviceServiceServer(grpcServer, NewServer(deviceURL))

	listen, err := net.Listen("tcp", *grpcEndpoint)
	if err != nil {
		log.Fatal(err)
	}
	log.Printf("[main] Starting gRPC Listener [%s]\n", *grpcEndpoint)
	log.Fatal(grpcServer.Serve(listen))
}
