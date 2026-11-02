import { formatReason } from "./i18n";

export function formatReasonList(reason: { code: string; args: Record<string, string> }): string {
  return formatReason(reason);
}
