import { HomeLayout } from 'fumadocs-ui/layouts/home';
import type { ReactNode } from 'react';

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <HomeLayout
      nav={{
        title: (
          <span className="flex items-center gap-2 font-bold">
            <span className="text-lg">⚡</span> Atlas
          </span>
        ),
      }}
      links={[
        { text: 'Docs', url: '/docs' },
        {
          text: 'GitHub',
          url: 'https://github.com/codeatlasdev/atlas',
          external: true,
        },
      ]}
    >
      {children}
    </HomeLayout>
  );
}
