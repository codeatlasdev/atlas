CREATE TYPE "public"."runtime_type" AS ENUM('k3s', 'swarm');--> statement-breakpoint
ALTER TABLE "servers" ADD COLUMN "runtime" "runtime_type" DEFAULT 'k3s' NOT NULL;