import { Hono } from "hono";
import { createUserSchema, updateUserSchema, userListQuerySchema } from "../schemas/user.js";
import { paginationSchema, idParamSchema, buildPaginatedResponse } from "../schemas/common.js";
import { validateBody, validateQuery } from "../middleware/validate.js";
import { formatNotFoundError, formatZodError, isZodError } from "../utils/errors.js";

const users = new Hono();

// In-memory store for demo purposes
const store = new Map<string, Record<string, unknown>>();

users.get("/", validateQuery(userListQuerySchema), async (c) => {
  const filters = c.get("validatedQuery");
  let query: Record<string, string>;
  try {
    query = c.req.query();
  } catch {
    query = {};
  }

  let pagination;
  try {
    pagination = paginationSchema.parse({
      page: query.page ? Number(query.page) : undefined,
      limit: query.limit ? Number(query.limit) : undefined,
    });
  } catch (error) {
    if (isZodError(error)) {
      return c.json(formatZodError(error), 400);
    }
    throw error;
  }

  let allUsers = Array.from(store.values());

  if (filters.role) {
    allUsers = allUsers.filter((u) => u.role === filters.role);
  }
  if (filters.search) {
    const term = filters.search.toLowerCase();
    allUsers = allUsers.filter(
      (u) =>
        String(u.name).toLowerCase().includes(term) ||
        String(u.email).toLowerCase().includes(term)
    );
  }

  const total = allUsers.length;
  const start = (pagination.page - 1) * pagination.limit;
  const items = allUsers.slice(start, start + pagination.limit);

  return c.json(buildPaginatedResponse(items, pagination, total));
});

users.get("/:id", async (c) => {
  let params;
  try {
    params = idParamSchema.parse({ id: c.req.param("id") });
  } catch (error) {
    if (isZodError(error)) {
      return c.json(formatZodError(error), 400);
    }
    throw error;
  }

  const user = store.get(params.id);
  if (!user) {
    return c.json(formatNotFoundError("User", params.id), 404);
  }
  return c.json({ data: user });
});

users.post("/", validateBody(createUserSchema), async (c) => {
  const body = c.get("validatedBody");
  const id = crypto.randomUUID();
  const now = new Date().toISOString();
  const user = { ...body, id, createdAt: now, updatedAt: now };
  store.set(id, user);
  return c.json({ data: user }, 201);
});

users.put("/:id", validateBody(updateUserSchema), async (c) => {
  let params;
  try {
    params = idParamSchema.parse({ id: c.req.param("id") });
  } catch (error) {
    if (isZodError(error)) {
      return c.json(formatZodError(error), 400);
    }
    throw error;
  }

  const existing = store.get(params.id);
  if (!existing) {
    return c.json(formatNotFoundError("User", params.id), 404);
  }

  const body = c.get("validatedBody");
  const updated = { ...existing, ...body, updatedAt: new Date().toISOString() };
  store.set(params.id, updated);
  return c.json({ data: updated });
});

users.delete("/:id", async (c) => {
  let params;
  try {
    params = idParamSchema.parse({ id: c.req.param("id") });
  } catch (error) {
    if (isZodError(error)) {
      return c.json(formatZodError(error), 400);
    }
    throw error;
  }

  if (!store.has(params.id)) {
    return c.json(formatNotFoundError("User", params.id), 404);
  }

  store.delete(params.id);
  return c.json({ success: true }, 204);
});

export { users };
