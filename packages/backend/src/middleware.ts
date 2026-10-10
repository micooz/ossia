// CORS preflight has to be answered here: Next handles OPTIONS for a route before the route module
// runs, so `withApi`'s OPTIONS branch is never reached and the browser sees a headerless 204.
// Actual requests keep their CORS headers from `withApi`, which is also where auth lives. The origin
// allowlist is shared through origins.ts, which imports nothing Node-specific.
import { NextResponse } from 'next/server';

import { corsFor } from './server/origins.ts';

export const config = { matcher: ['/api/:path*'] };

export function middleware(req: Request): Response {
  if (req.method !== 'OPTIONS') return NextResponse.next();
  return new NextResponse(null, { status: 204, headers: corsFor(req.headers.get('origin')) });
}
