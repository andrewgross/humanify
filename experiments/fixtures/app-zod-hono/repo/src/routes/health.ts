import { Hono } from "hono";

const health = new Hono();

health.get("/", (c) => {
  return c.json({
    status: "ok",
    timestamp: new Date().toISOString(),
    uptime: process.uptime(),
  });
});

health.get("/ready", (c) => {
  // Could check DB connections, external services, etc.
  const checks = {
    memory: process.memoryUsage().heapUsed < 500 * 1024 * 1024,
    uptime: process.uptime() > 1,
  };
  const allHealthy = Object.values(checks).every(Boolean);
  return c.json(
    { status: allHealthy ? "ready" : "degraded", checks },
    allHealthy ? 200 : 503
  );
});

export { health };
