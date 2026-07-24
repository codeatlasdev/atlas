import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	site: 'https://atlas.codeatlas.com.br',
	integrations: [
		starlight({
			title: 'Atlas',
			logo: {
				src: './src/assets/icon.svg',
			},
			social: [
				{ icon: 'github', label: 'GitHub', href: 'https://github.com/codeatlasdev/atlas' },
			],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quick Start', slug: 'getting-started/quick-start' },
						{ label: 'Configuration', slug: 'getting-started/configuration' },
					],
				},
				{
					label: 'TUI',
					items: [
						{ label: 'Overview', slug: 'tui/overview' },
						{ label: 'Keyboard Shortcuts', slug: 'tui/shortcuts' },
						{ label: 'Log Management', slug: 'tui/logs' },
						{ label: 'Command Palette', slug: 'tui/command-palette' },
					],
				},
				{
					label: 'CLI',
					items: [
						{ label: 'Commands', slug: 'cli/commands' },
						{ label: 'Self Update', slug: 'cli/self-update' },
						{ label: 'Headless Mode', slug: 'cli/headless' },
					],
				},
				{
					label: 'Architecture',
					items: [
						{ label: 'Overview', slug: 'architecture/overview' },
						{ label: 'Crate Map', slug: 'architecture/crates' },
						{ label: 'Distribution', slug: 'architecture/distribution' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'atlas.yaml', slug: 'reference/config' },
						{ label: 'Environment', slug: 'reference/environment' },
					],
				},
			],
		}),
	],
});
