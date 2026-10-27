import { describe, expect, it } from 'vitest';

import { OverviewLoads } from './reload';

describe('OverviewLoads', () => {
	it('shows responses that arrive in order', () => {
		const loads = new OverviewLoads();
		const first = loads.start(false);
		expect(loads.settle(first)).toEqual({ apply: true, reopen: false });
		const second = loads.start(true);
		expect(loads.settle(second)).toEqual({ apply: true, reopen: true });
	});

	it('drops a slow response once a newer one is shown', () => {
		const loads = new OverviewLoads();
		const slow = loads.start(false);
		const fast = loads.start(false);

		expect(loads.settle(fast).apply).toBe(true);
		expect(loads.settle(slow).apply).toBe(false);
	});

	it('reopens the file with a newer response when the load that asked for it is superseded', () => {
		const loads = new OverviewLoads();
		const sourceChange = loads.start(true);
		const viewedMark = loads.start(false);

		expect(loads.settle(viewedMark)).toEqual({ apply: true, reopen: true });
		expect(loads.settle(sourceChange)).toEqual({ apply: false, reopen: false });
	});

	it('shows an older response when the newer load fails', () => {
		const loads = new OverviewLoads();
		const sourceChange = loads.start(true);
		loads.start(false); // fails, so it never settles

		expect(loads.settle(sourceChange)).toEqual({ apply: true, reopen: true });
	});

	it('keeps a reopen pending past a failed load until a response is shown', () => {
		const loads = new OverviewLoads();
		loads.start(true); // fails
		const later = loads.start(false);

		expect(loads.settle(later)).toEqual({ apply: true, reopen: true });
		expect(loads.settle(loads.start(false))).toEqual({ apply: true, reopen: false });
	});

	it('does not reopen for a response older than the load that asked', () => {
		const loads = new OverviewLoads();
		const viewedMark = loads.start(false);
		const sourceChange = loads.start(true);

		expect(loads.settle(viewedMark)).toEqual({ apply: true, reopen: false });
		expect(loads.settle(sourceChange)).toEqual({ apply: true, reopen: true });
	});
});
