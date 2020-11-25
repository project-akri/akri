package main

import (
	"context"
	"flag"
	"log"
	"time"

	pb "github.com/deislabs/akri/http-extensibility/proto"

	"google.golang.org/grpc"
)

var (
	grpcEndpoint = flag.String("grpc_endpoint", "", "The endpoint of the gRPC server.")
)

func main() {
	log.Println("[main] Starting gRPC client")
	defer func() {
		log.Println("[main] Stopping gRPC client")
	}()

	flag.Parse()
	if *grpcEndpoint == "" {
		log.Fatal("[main] Unable to start client. Requires endpoint to a gRPC Server.")
	}

	dialOpts := []grpc.DialOption{
		grpc.WithInsecure(),
	}
	log.Printf("Connecting to gRPC server [%s]", *grpcEndpoint)
	conn, err := grpc.Dial(*grpcEndpoint, dialOpts...)
	if err != nil {
		log.Fatal(err)
	}
	defer conn.Close()

	client := pb.NewDeviceServiceClient(conn)
	ctx := context.Background()

	for {
		log.Println("[main:loop]")

		// Call Service
		{
			rqst := &pb.ReadSensorRequest{
				Name: "/",
			}
			log.Println("[main:loop] Calling read_sensor")
			resp, err := client.ReadSensor(ctx, rqst)
			if err != nil {
				log.Fatal(err)
			}

			log.Printf("[main:loop] Success: %+v", resp)
		}

		// Add a pause between iterations
		log.Println("[main:loop] Sleep")
		time.Sleep(10 * time.Second)
	}
}
