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

// Build Info
var (
	BuildDate string
	BuildUser string
	Version   string
)

var _ flag.Value = (*shared.RepeatableFlag)(nil)
var devices shared.RepeatableFlag

func main() {
	log.Printf("[main] Version: %s", Version)
	log.Printf("[main] Build user: %s", BuildUser)
	log.Printf("[main] Build date: %s", BuildDate)

	flag.Var(&devices, "device", "Repeat this flag to add devices to the discovery service")
	flag.Parse()

	// Handlers: Devices, Healthz
	handler := http.NewServeMux()
	handler.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		log.Println("[main:healthz] Handler entered")
		fmt.Fprint(w, "ok")
	})
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
func healthz(w http.ResponseWriter, r *http.Request) {
	fmt.Fprint(w, "ok")
}
