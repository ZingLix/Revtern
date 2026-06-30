import { RevternApi } from "@revtern/api-client";

export const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? "";
export const api = new RevternApi(API_BASE_URL);
