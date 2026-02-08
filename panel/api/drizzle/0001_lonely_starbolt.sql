ALTER TABLE "organizations" ADD COLUMN "github_app_id" integer;--> statement-breakpoint
ALTER TABLE "organizations" ADD COLUMN "github_client_id" varchar(255);--> statement-breakpoint
ALTER TABLE "organizations" ADD COLUMN "github_client_secret_enc" text;