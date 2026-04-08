import { HomeLayout } from 'fumadocs-ui/layouts/home';
import { Logo } from '@/components/logo';
import type { ReactNode } from 'react';

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <HomeLayout
      nav={{
        title: (
          <span className="flex items-center gap-2 font-bold">
            <Logo size={22} /> Atlas
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
