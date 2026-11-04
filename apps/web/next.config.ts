import type { NextConfig } from "next";
import path from "node:path";

const nextConfig: NextConfig = {
  output: 'standalone',
  outputFileTracingRoot: path.resolve(__dirname, '../..'),
  outputFileTracingExcludes: {
    '/*': [
      '../../data/**/*', '../../user/**/*', '../../.env', '../../.env.*', './.env*',
    ],
  },
  images: { unoptimized: true },
  turbopack: { root: path.resolve(__dirname, "../..") },
  poweredByHeader: false,
  transpilePackages: ["@ai-phone/protocol"],
  async rewrites() {
    return {beforeFiles:['stream','playback'].map(operation=>({source:`/api/voice/${operation}`,destination:`http://127.0.0.1:17863/v1/voice/${operation}`}))};
  },
  async headers() {
    return [{ source: '/:path*', headers: [
      { key: 'X-Content-Type-Options', value: 'nosniff' },
      { key: 'X-Frame-Options', value: 'DENY' },
      { key: 'Referrer-Policy', value: 'no-referrer' },
      { key: 'Content-Security-Policy', value: "frame-ancestors 'none'; object-src 'none'; base-uri 'self'" },
    ] }];
  },
};

export default nextConfig;

