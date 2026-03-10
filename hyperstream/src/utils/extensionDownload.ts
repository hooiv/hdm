export function buildExtensionDownloadHeaders(
  customHeaders?: Record<string, string> | null,
  pageUrl?: string | null,
): Record<string, string> | undefined {
  const effectiveHeaders = Object.fromEntries(
    Object.entries(customHeaders || {}).filter(([key, value]) => {
      return key.trim().length > 0 && typeof value === 'string' && value.trim().length > 0;
    }),
  );

  const hasReferer = Object.keys(effectiveHeaders).some((key) => key.toLowerCase() === 'referer');
  const normalizedPageUrl = pageUrl?.trim();

  if (!hasReferer && normalizedPageUrl) {
    effectiveHeaders.Referer = normalizedPageUrl;
  }

  return Object.keys(effectiveHeaders).length > 0 ? effectiveHeaders : undefined;
}