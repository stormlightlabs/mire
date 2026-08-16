import { base } from '$app/paths';
import { getDocs } from '$lib/content';

export const prerender = true;

export function GET(): Response {
	const pages = getDocs().map((doc) => `- [${doc.title}](${base}/docs/${doc.slug}.md): ${doc.description}`);
	const body = [
		'# Mire',
		'',
		'> A terminal difftool for humans and agents.',
		'',
		'Mire reviews Git and patch changesets, renders them in a continuous terminal stream, and stores durable anchored findings in review files. Use these pages to install Mire, operate the viewer, exchange review notes, and understand its data model.',
		'',
		'## Documentation',
		'',
		...pages,
		''
	].join('\n');

	return new Response(body, { headers: { 'content-type': 'text/plain; charset=utf-8' } });
}
