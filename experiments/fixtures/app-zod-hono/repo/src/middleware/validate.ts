import type { Context, Next } from "hono";
import type { ZodSchema } from "zod";
import { ZodError } from "zod";
import { formatZodError } from "../utils/errors.js";

export function validateBody<T>(schema: ZodSchema<T>) {
  return async (c: Context, next: Next) => {
    try {
      const body = await c.req.json();
      const parsed = schema.parse(body);
      c.set("validatedBody", parsed);
      await next();
    } catch (error) {
      if (error instanceof ZodError) {
        const formatted = formatZodError(error);
        return c.json(formatted, 400);
      }
      throw error;
    }
  };
}

export function validateQuery<T>(schema: ZodSchema<T>) {
  return async (c: Context, next: Next) => {
    try {
      const query = c.req.query();
      const parsed = schema.parse(query);
      c.set("validatedQuery", parsed);
      await next();
    } catch (error) {
      if (error instanceof ZodError) {
        const formatted = formatZodError(error);
        return c.json(formatted, 400);
      }
      throw error;
    }
  };
}
