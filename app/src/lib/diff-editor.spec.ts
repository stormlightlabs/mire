import { describe, expect, test } from 'vitest';
import { hunkDocuments } from './diff-editor';

describe('hunkDocuments', () => {
	test('reconstructs before and after documents from patch markers', () => {
		expect(hunkDocuments({ lines: [' unchanged\n', '-old handler\n', '+bounded handler\n', ' return nil\n'] })).toEqual(
			{ before: 'unchanged\nold handler\nreturn nil\n', after: 'unchanged\nbounded handler\nreturn nil\n' }
		);
	});

	test('preserves markerless content and absent sides', () => {
		expect(hunkDocuments({ lines: ['+first', '+second'] })).toEqual({ before: '', after: 'firstsecond' });
		expect(hunkDocuments({ lines: ['metadata'] })).toEqual({ before: 'metadata', after: 'metadata' });
	});
});
