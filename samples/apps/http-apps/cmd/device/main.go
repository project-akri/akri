package main

import (
	"flag"
	"fmt"
	"log"
	"math/rand"
	"net"
	"net/http"
	"time"

	"github.com/deislabs/akri/http-extensibility/shared"
)

const (
	addr = ":8080"
)

var _ flag.Value = (*shared.RepeatableFlag)(nil)
var paths shared.RepeatableFlag

func main() {
	flag.Var(&paths, "path", "Repeat this flag to add paths for the device")
	flag.Parse()

	// At a minimum, respond on `/`
	if len(paths) == 0 {
		paths = []string{"/"}
	}
	log.Printf("[main] Paths: %d", len(paths))

	seed := rand.NewSource(time.Now().UnixNano())
	entr := rand.New(seed)

	handler := http.NewServeMux()

	// Create handler for each endpoint
	for _, path := range paths {
		log.Printf("[main] Creating handler: %s", path)
		handler.HandleFunc(path, func(w http.ResponseWriter, r *http.Request) {
			log.Printf("[main:handler] Handler entered: %s", path)
			fmt.Fprint(w, entr.Float64())
		})
	}

	s := &http.Server{
		Addr:    addr,
		Handler: handler,
	}
	listen, err := net.Listen("tcp", addr)
	if err != nil {
		log.Fatal(err)
	}

	log.Printf("[main] Starting Device: [%s]", addr)
	log.Fatal(s.Serve(listen))
}
