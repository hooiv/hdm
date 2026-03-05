// simple debug logger that only emits in development
export function debug(...args: any[]) {
  if (import.meta.env.DEV) {
    // console.debug preferred for filtering
    console.debug(...args);
  }
}

export function info(...args: any[]) {
  console.info(...args);
}

export function warn(...args: any[]) {
  console.warn(...args);
}

export function error(...args: any[]) {
  console.error(...args);
}
