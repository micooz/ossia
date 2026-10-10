// This service serves only `app/api/**`; the product UI is the platform-agnostic SPA in
// packages/ui. Next still needs a root layout to build the built-in error page, so this is the
// whole "frontend" of the API host — deliberately inert.
export const metadata = { title: 'Ossia API', robots: { index: false, follow: false } };

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
