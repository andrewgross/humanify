import { z } from "zod";

export const createUserSchema = z.object({
  name: z.string().min(1).max(100),
  email: z.string().email(),
  age: z.number().int().min(0).max(150).optional(),
  role: z.enum(["admin", "user", "moderator"]).default("user"),
});

export type CreateUser = z.infer<typeof createUserSchema>;

export const updateUserSchema = createUserSchema.partial().extend({
  id: z.string().uuid(),
});

export type UpdateUser = z.infer<typeof updateUserSchema>;

export const userResponseSchema = createUserSchema.extend({
  id: z.string().uuid(),
  createdAt: z.string().datetime(),
  updatedAt: z.string().datetime(),
});

export type UserResponse = z.infer<typeof userResponseSchema>;

export const userListQuerySchema = z.object({
  role: z.enum(["admin", "user", "moderator"]).optional(),
  search: z.string().optional(),
  sortBy: z.enum(["name", "email", "createdAt"]).default("createdAt"),
  order: z.enum(["asc", "desc"]).default("desc"),
});

export type UserListQuery = z.infer<typeof userListQuerySchema>;

export function sanitizeUserResponse(raw: Record<string, unknown>): UserResponse {
  return userResponseSchema.parse(raw);
}

export function validateUserFilters(query: Record<string, unknown>): UserListQuery {
  return userListQuerySchema.parse(query);
}
