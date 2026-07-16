export async function getJSON<T>(path: string, signal?: AbortSignal): Promise<T> {
	const response = await fetch(path, { signal, headers: { Accept: 'application/json' } });
	if (!response.ok) throw new Error(`${path} returned ${response.status}.`);
	return (await response.json()) as T;
}
