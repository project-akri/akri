package main

import (
	"flag"
	"fmt"
	"html"
	"log"
	"net"
	"net/http"

	"github.com/deislabs/akri/http-extensibility/shared"
)

const (
	addr = ":9999"
)

var _ flag.Value = (*shared.RepeatableFlag)(nil)
var devices shared.RepeatableFlag

func main() {
	flag.Var(&devices, "device", "Repeat this flag to add devices to the discovery service")
	flag.Parse()

	handler := http.NewServeMux()
	handler.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		log.Printf("[discovery] Handler entered")
		fmt.Fprintf(w, "%s\n", html.EscapeString(devices.String()))
	})

	s := &http.Server{
		Addr:    addr,
		Handler: handler,
	}
	listen, err := net.Listen("tcp", addr)
	if err != nil {
		log.Fatal(err)
	}

	log.Printf("[createDiscoveryService] Starting Discovery Service: %s", addr)
	log.Fatal(s.Serve(listen))
}
