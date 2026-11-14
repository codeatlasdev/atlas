package main

import (
	"context"
	"log"
	"net/http"
	"os"
)

func main() {
	project := os.Getenv("FCM_PROJECT_ID")
	if project == "" {
		log.Fatal("FCM_PROJECT_ID is required")
	}

	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	keys, err := loadKeys(os.Getenv("RELAY_GRANT_KEY"), os.Getenv("RELAY_GRANT_KEY_PREVIOUS"))
	if err != nil {
		log.Fatal(err)
	}

	getToken, err := newGoogleTokenSource(context.Background())
	if err != nil {
		log.Fatalf("credentials: %v", err)
	}

	r := newRelay(project, "https://fcm.googleapis.com", getToken)
	r.keys = keys
	r.trustProxy = os.Getenv("RELAY_TRUST_PROXY") == "1"
	log.Printf("push-relay listening on :%s (project=%s)", port, project)
	if err := http.ListenAndServe(":"+port, r); err != nil {
		log.Fatal(err)
	}
}
