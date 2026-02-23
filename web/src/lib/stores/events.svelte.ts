import { untrack } from 'svelte';
import { connectEvents, type EventMessage } from '$lib/api';

const MAX_EVENTS = 50;

let events = $state<EventMessage[]>([]);
let connected = $state(false);
let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelay = 500;

function handleMessage(ev: MessageEvent) {
	try {
		const msg: EventMessage = JSON.parse(ev.data);
		// untrack prevents this async callback from accidentally registering
		// as a dependency or interfering with Svelte's scheduler during navigation
		untrack(() => {
			// Deduplicate by seq — guards against server replay overlap
			if (events.some((e) => e.seq === msg.seq)) return;
			events = [msg, ...events].slice(0, MAX_EVENTS);
		});
	} catch {
		// ignore malformed messages
	}
}

function handleOpen() {
	connected = true;
	reconnectDelay = 500;
}

function handleClose() {
	connected = false;
	ws = null;
	scheduleReconnect();
}

function scheduleReconnect() {
	if (reconnectTimer) return;
	reconnectTimer = setTimeout(() => {
		reconnectTimer = null;
		connect();
	}, reconnectDelay);
	reconnectDelay = Math.min(reconnectDelay * 1.5, 5000);
}

export function connect() {
	if (ws) return;
	const lastSeq = events.length > 0 ? events[0].seq : 0;
	const socket = connectEvents();
	ws = socket;
	socket.onmessage = handleMessage;
	socket.onopen = () => {
		handleOpen();
		// Send after_seq so the server replays only events we haven't seen
		if (lastSeq > 0) socket.send(JSON.stringify({ after_seq: lastSeq }));
	};
	socket.onclose = handleClose;
	socket.onerror = () => socket.close();
}

export function disconnect() {
	if (reconnectTimer) {
		clearTimeout(reconnectTimer);
		reconnectTimer = null;
	}
	if (ws) {
		ws.onclose = null;
		ws.close();
		ws = null;
	}
	connected = false;
}

export function getEvents(): EventMessage[] {
	return events;
}

export function isConnected(): boolean {
	return connected;
}
