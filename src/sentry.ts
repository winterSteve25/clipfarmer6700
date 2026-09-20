import * as Sentry from "@sentry/react";

export function initializeSentry() {
  Sentry.init({
    dsn: "https://434109640748b44d5c81e629289a3bad@o4512118777839616.ingest.us.sentry.io/4512118779936768",
    sendDefaultPii: false,
  });
}

export function captureFrontendError(error: unknown, operation: string) {
  const exception = error instanceof Error ? error : new Error(String(error));

  Sentry.withScope((scope) => {
    scope.setTag("area", "frontend");
    scope.setTag("operation", operation);
    if (!(error instanceof Error)) scope.setExtra("originalError", error);
    Sentry.captureException(exception);
  });
}
