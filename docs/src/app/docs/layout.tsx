import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import type { ReactNode } from 'react';

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <DocsLayout
      tree={source.getPageTree()}
      nav={{
        title: (
          <span className="flex items-center gap-2 font-bold">
            <span className="text-lg">⚡</span> Atlas
          </span>
        ),
      }}
      links={[
        {
          text: 'GitHub',
          url: 'https://github.com/codeatlasdev/atlas',
          external: true,
        },
      ]}
    >
      {children}
    </DocsLayout>
  );
}
