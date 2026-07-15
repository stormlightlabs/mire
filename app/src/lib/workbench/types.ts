export type Session = {
	id: string;
	repository_name: string;
	title: string;
	created_at: string;
	current_round_id?: string;
};

export type Round = { id: string; number: number; status: string; snapshot_id?: string; created_at: string };

export type Activity = { activity_id: number; event_kind: string; message: string; created_at: string };

export type Capabilities = { review_data: boolean; actions: boolean; sse: boolean };

export type Bootstrap = {
	schema_version: string;
	authenticated: boolean;
	sessions: Session[];
	selected_session: Session | null;
	current_round: Round | null;
	capabilities: Capabilities;
};

export type LoadState = 'loading' | 'ready' | 'unauthenticated' | 'offline' | 'error';
