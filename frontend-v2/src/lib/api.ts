import type {
  CreateTableRequest,
  CreateTableResponse,
} from "../types/protocol";
import { getApiBaseUrl } from "./env";

export class ApiError extends Error {
  status: number;
  detail: string;
  constructor(status: number, detail: string) {
    super(detail);
    this.status = status;
    this.detail = detail;
  }
}

async function parseDetail(res: Response): Promise<string> {
  try {
    const data = await res.json();
    if (typeof data?.detail === "string") return data.detail;
    return JSON.stringify(data);
  } catch {
    return res.statusText || `http_${res.status}`;
  }
}

export async function createTable(
  body: CreateTableRequest,
): Promise<CreateTableResponse> {
  const res = await fetch(`${getApiBaseUrl()}/api/tables`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new ApiError(res.status, await parseDetail(res));
  }
  return (await res.json()) as CreateTableResponse;
}

export async function healthCheck(): Promise<boolean> {
  try {
    const res = await fetch(`${getApiBaseUrl()}/api/health`);
    return res.ok;
  } catch {
    return false;
  }
}
