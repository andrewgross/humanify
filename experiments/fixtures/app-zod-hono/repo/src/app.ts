import { Hono } from "hono";
import { logger } from "hono/logger";
import { users } from "./routes/users.js";
import { health } from "./routes/health.js";
import { formatServerError, isZodError, formatZodError } from "./utils/errors.js";

export function createApp(): Hono {
  const app = new Hono();

  // Request logging
  app.use("*", logger());

  // Global error handler
  app.onError((err, c) => {
    if (isZodError(err)) {
      return c.json(formatZodError(err), 400);
    }
    console.error("Unhandled error:", err);
    return c.json(formatServerError(err.message), 500);
  });

  // Not found handler
  app.notFound((c) => {
    return c.json({ error: "Not found", code: 404 }, 404);
  });

  // Mount routes
  app.route("/api/users", users);
  app.route("/api/health", health);

  return app;
}
