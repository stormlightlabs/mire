export const site = {
	name: 'Mire',
	title: 'Mire - Review code together',
	description: 'Mire is a local, terminal-based code review environment for humans and agents.',
	url: 'https://mire.stormlightlabs.org',
	imagePath: '/og.png',
	imageAlt: 'Mire code review documentation.'
} as const;

export function absoluteUrl(pathname: string): string {
	return new URL(pathname, site.url).toString();
}
