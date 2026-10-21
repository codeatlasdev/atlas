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

	getToken, err := newGoogleTokenSource(context.Background())
	if err != nil {
		log.Fatalf("credentials: %v", err)
	}

	r := newRelay(project, "https://fcm.googleapis.com", getToken)
	log.Printf("push-relay listening on :%s (project=%s)", port, project)
	if err := http.ListenAndServe(":"+port, r); err != nil {
		log.Fatal(err)
	}
}
