import { writable } from "svelte/store";

export interface Confirmation {
  title: string;
  message: string;
  confirmLabel: string;
  dangerous: boolean;
  /** Acknowledge-only: the Cancel button is hidden and there is nothing to decline. */
  noticeOnly: boolean;
}

export const confirmPrompt = writable<Confirmation | null>(null);
let pending: ((accepted: boolean) => void) | null = null;

export function requestConfirmation(
  message: string,
  options: Partial<Omit<Confirmation, "message">> = {},
): Promise<boolean> {
  if (pending) pending(false);
  confirmPrompt.set({
    title: options.title ?? "Confirm action",
    message,
    confirmLabel: options.confirmLabel ?? "Continue",
    dangerous: options.dangerous ?? false,
    noticeOnly: options.noticeOnly ?? false,
  });
  return new Promise((resolve) => {
    pending = resolve;
  });
}

/** Same surface as `requestConfirmation`, for messages that only need acknowledging. */
export function showNotice(
  message: string,
  options: Partial<Omit<Confirmation, "message" | "noticeOnly">> = {},
): Promise<boolean> {
  return requestConfirmation(message, {
    title: options.title ?? "Notice",
    confirmLabel: options.confirmLabel ?? "OK",
    dangerous: options.dangerous ?? false,
    noticeOnly: true,
  });
}

export function resolveConfirmation(accepted: boolean) {
  const resolve = pending;
  pending = null;
  confirmPrompt.set(null);
  resolve?.(accepted);
}
