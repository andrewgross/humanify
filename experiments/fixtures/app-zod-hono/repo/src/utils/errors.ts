import { ZodError } from "zod";
import type { ErrorResponse } from "../schemas/common.js";

export function formatZodError(error: ZodError): ErrorResponse {
  const details = error.issues.map((issue) => {
    const path = issue.path.join(".");
    return path ? `${path}: ${issue.message}` : issue.message;
  });

  return {
    error: "Validation failed",
    details,
    code: 400,
  };
}

export function formatNotFoundError(resource: string, id: string): ErrorResponse {
  return {
    error: `${resource} not found`,
    details: [`No ${resource} with id '${id}' exists`],
    code: 404,
  };
}

export function formatServerError(message: string): ErrorResponse {
  return {
    error: "Internal server error",
    details: [message],
    code: 500,
  };
}

export function isZodError(error: unknown): error is ZodError {
  return error instanceof ZodError;
}
