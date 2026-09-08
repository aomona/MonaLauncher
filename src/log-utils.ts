/** Redact credential-shaped values before storage, display, copy or export. */
export function redactLog(line: string): string {
  return line
    .replace(/(Bearer\s+)[\w.+/~=-]+/gi, "$1[redacted]")
    .replace(
      /((?:access[_-]?token|refresh[_-]?token|client[_-]?secret|authorization|--accessToken)["']?\s*[:= ]\s*["']?)[^\s,"'}]+/gi,
      "$1[redacted]",
    );
}
