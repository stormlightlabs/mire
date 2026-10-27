/** What to do with an overview response once it arrives. */
export type SettledLoad = {
	/** Whether to show this response. False when a newer one is already shown. */
	apply: boolean;
	/** Whether to load the open file again against this response. */
	reopen: boolean;
};

/**
 * Orders overview loads that overlap. Each load takes a ticket when it starts,
 * and its response is shown only when no newer response was shown first. A slow
 * response therefore cannot overwrite a newer one, and a newer load that fails
 * does not discard an older one that succeeds.
 *
 * A load can ask for the open file to load again, as a source change requires.
 * The request stays pending until a response from that load or a newer one is
 * shown, so it is not lost when a newer load that did not ask for it wins.
 */
export class OverviewLoads {
	#started = 0;
	#applied = 0;
	#reopens: number[] = [];

	/** Starts a load and returns its ticket. */
	start(reopen: boolean): number {
		const ticket = ++this.#started;
		if (reopen) this.#reopens.push(ticket);
		return ticket;
	}

	/** Settles a successful load. A failed load needs no call: its pending reopen waits for the next one shown. */
	settle(ticket: number): SettledLoad {
		if (ticket <= this.#applied) return { apply: false, reopen: false };
		this.#applied = ticket;
		const reopen = this.#reopens.some((pending) => pending <= ticket);
		this.#reopens = this.#reopens.filter((pending) => pending > ticket);
		return { apply: true, reopen };
	}
}
