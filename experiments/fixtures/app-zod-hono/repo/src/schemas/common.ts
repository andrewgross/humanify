import { z } from "zod";

export const paginationSchema = z.object({
  page: z.number().int().positive().default(1),
  limit: z.number().int().min(1).max(100).default(20),
});

export type Pagination = z.infer<typeof paginationSchema>;

export const errorResponseSchema = z.object({
  error: z.string(),
  details: z.array(z.string()).optional(),
  code: z.number().int(),
});

export type ErrorResponse = z.infer<typeof errorResponseSchema>;

export const idParamSchema = z.object({
  id: z.string().uuid(),
});

export function buildPaginatedResponse<T>(
  items: T[],
  pagination: Pagination,
  total: number
) {
  const totalPages = Math.ceil(total / pagination.limit);
  return {
    data: items,
    pagination: {
      page: pagination.page,
      limit: pagination.limit,
      total,
      totalPages,
      hasNext: pagination.page < totalPages,
      hasPrev: pagination.page > 1,
    },
  };
}
