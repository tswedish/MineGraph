<script lang="ts">
	import { connect, disconnect, getEvents, isConnected } from '$lib/stores/events.svelte';
	import type { EventMessage } from '$lib/api';

	let { maxEvents = 20 }: { maxEvents?: number } = $props();

	// Use $effect with cleanup instead of onMount/onDestroy — required for
	// reliable lifecycle in Svelte 5 runes-mode components with SvelteKit navigation.
	$effect(() => {
		connect();
		return () => disconnect();
	});

	const events = $derived(getEvents().slice(0, maxEvents));
	const connected = $derived(isConnected());

	function parsePayload(payload: string | object): Record<string, unknown> {
		try {
			return typeof payload === 'string' ? JSON.parse(payload) : (payload as Record<string, unknown>);
		} catch {
			return {};
		}
	}

	function typeColor(event: EventMessage): string {
		const t = event.event_type;
		if (t === 'challenge.created') return 'var(--color-accent)';
		if (t === 'record.updated') return '#f59e0b';
		if (t === 'graph.verified') {
			const p = parsePayload(event.payload);
			return p.verdict === 'accepted' ? 'var(--color-accepted)' : 'var(--color-rejected)';
		}
		if (t === 'graph.submitted') return 'var(--color-text)';
		return 'var(--color-text-muted)';
	}

	function typeLabel(event: EventMessage): string {
		const t = event.event_type;
		if (t === 'graph.verified') {
			const p = parsePayload(event.payload);
			return p.verdict === 'accepted' ? 'verified' : 'rejected';
		}
		if (t === 'challenge.created') return 'new challenge';
		if (t === 'graph.submitted') return 'submitted';
		if (t === 'record.updated') return 'new record';
		return t;
	}

	function detail(event: EventMessage): string {
		const p = parsePayload(event.payload);
		const cid = p.challenge_id as string | undefined;
		switch (event.event_type) {
			case 'challenge.created':
				return `${cid}  R(${p.k},${p.ell})`;
			case 'graph.submitted':
				return `${cid}  n=${p.n}`;
			case 'graph.verified': {
				const parts: string[] = [cid ?? ''];
				if (p.n != null) parts.push(`n=${p.n}`);
				if (p.reason) parts.push(String(p.reason));
				return parts.join('  ');
			}
			case 'record.updated':
				return `${cid}  n=${p.best_n}`;
			default: {
				if (cid) return cid;
				return JSON.stringify(p).slice(0, 40);
			}
		}
	}

	function getCid(event: EventMessage): string | null {
		const p = parsePayload(event.payload);
		return (p.graph_cid as string) ?? (p.best_cid as string) ?? null;
	}

	function timeAgo(iso: string): string {
		const now = Date.now();
		const then = new Date(iso).getTime();
		const sec = Math.floor((now - then) / 1000);
		if (sec < 5) return 'just now';
		if (sec < 60) return `${sec}s ago`;
		const min = Math.floor(sec / 60);
		if (min < 60) return `${min}m ago`;
		const hr = Math.floor(min / 60);
		if (hr < 24) return `${hr}h ago`;
		const d = Math.floor(hr / 24);
		return `${d}d ago`;
	}
</script>

<div class="event-feed">
	<div class="feed-header">
		<span class="feed-title">Live Events</span>
		<span class="connection-dot" class:online={connected}></span>
	</div>
	{#if events.length === 0}
		<div class="empty">No events yet</div>
	{:else}
		<div class="events-list">
			{#each events as event (event.seq)}
				<div class="event-row">
					<span class="seq">#{event.seq}</span>
					<span class="event-type" style="color: {typeColor(event)}">
						{typeLabel(event)}
					</span>
					{#if getCid(event)}
					<a class="detail detail-link" href="/submissions/{getCid(event)}">{detail(event)}</a>
				{:else}
					<span class="detail">{detail(event)}</span>
				{/if}
					<span class="time">{timeAgo(event.created_at)}</span>
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.event-feed {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 0.75rem;
		overflow: hidden;
	}

	.feed-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border);
	}

	.feed-title {
		font-family: var(--font-mono);
		font-size: 0.8125rem;
		font-weight: 600;
	}

	.connection-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-rejected);
	}

	.connection-dot.online {
		background: var(--color-accepted);
	}

	.empty {
		padding: 1.5rem 1rem;
		text-align: center;
		color: var(--color-text-muted);
		font-size: 0.8125rem;
	}

	.events-list {
		max-height: 320px;
		overflow-y: auto;
	}

	.event-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.375rem 1rem;
		font-family: var(--font-mono);
		font-size: 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}

	.event-row:last-child {
		border-bottom: none;
	}

	.seq {
		color: var(--color-text-muted);
		min-width: 2.5rem;
	}

	.event-type {
		font-weight: 600;
		min-width: 8rem;
	}

	.detail {
		color: var(--color-text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}

	.detail-link {
		text-decoration: none;
	}

	.detail-link:hover {
		color: var(--color-accent);
	}

	.time {
		color: var(--color-text-muted);
		opacity: 0.6;
		white-space: nowrap;
		margin-left: auto;
		font-size: 0.6875rem;
	}
</style>
