import Link from 'next/link';

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col items-center justify-center px-4 text-center">
      <div className="mb-4 inline-flex items-center rounded-full border px-3 py-1 text-sm text-fd-muted-foreground">
        Open Source · Self-hosted · Any VPS
      </div>

      <h1 className="mb-4 max-w-2xl text-4xl font-bold tracking-tight sm:text-5xl">
        Your own Heroku.{' '}
        <span className="text-[#6366f1]">One command.</span>
      </h1>

      <p className="mb-8 max-w-lg text-lg text-fd-muted-foreground">
        Atlas turns any Linux server into a production-ready platform.
        Developers write code — Atlas handles infrastructure, DNS, HTTPS,
        secrets, and deployments.
      </p>

      <div className="flex flex-col gap-3 sm:flex-row">
        <Link
          href="/docs"
          className="rounded-lg bg-[#6366f1] px-6 py-2.5 text-sm font-medium text-white transition-colors hover:bg-[#4f46e5]"
        >
          Get Started
        </Link>
        <Link
          href="https://github.com/codeatlasdev/atlas"
          className="rounded-lg border px-6 py-2.5 text-sm font-medium transition-colors hover:bg-fd-accent"
        >
          GitHub
        </Link>
      </div>

      <div className="mt-12 w-full max-w-xl rounded-lg border bg-fd-card p-4 text-left">
        <pre className="overflow-x-auto text-sm">
          <code>
            <span className="text-fd-muted-foreground">$</span> curl -fsSL https://atlas.codeatlas.com.br/install.sh | bash{'\n'}
            <span className="text-fd-muted-foreground">$</span> atlas login{'\n'}
            <span className="text-fd-muted-foreground">$</span> atlas infra setup --host root@your-server.com{'\n'}
            <span className="text-fd-muted-foreground">$</span> atlas deploy{'\n'}
            <span className="text-[#6366f1]">✓</span> Live at https://myapp.com
          </code>
        </pre>
      </div>
    </main>
  );
}
