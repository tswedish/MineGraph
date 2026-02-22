import { untrack } from 'svelte';
import { connectEvents, type EventMessage } from '$lib/api';

const MAX_EVENTS = 50;

let events = $state<EventMessage[]>([]);
let connected = $state(false);
let ws: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let reconnectDelay = 1000;

function handleMessage(ev: MessageEvent) {
	try {
		const msg: EventMessage = JSON.parse(ev.data);
		// untrack prevents this async callback from accidentally registering
		// as a dependency or interfering with Svelte's scheduler during navigation
		untrack(() => {
			events = [msg, ...events].slice(0, MAX_EVENTS);
		});
	} catch {
		// ignore malformed messages
	}
}

function handleOpen() {
	connected = true;
	reconnectDelay = 1000;
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
	reconnectDelay = Math.min(reconnectDelay * 2, 30000);
}

export function connect() {
	if (ws) return;
	const lastSeq = events.length > 0 ? events[0].seq : 0;
	ws = connectEvents(lastSeq);
	ws.onmessage = handleMessage;
	ws.onopen = handleOpen;
	ws.onclose = handleClose;
	ws.onerror = () => ws?.close();
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
