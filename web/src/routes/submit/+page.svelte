<script lang="ts">
	import GemView from '$lib/components/GemView.svelte';
	import { decodeGraph6, edgeCount } from '@extremal/shared';
	import {
		verifyGraph, submitGraph, registerKey, getLeaderboard, getSubmission,
		type VerifyResponse, type SubmitResponse, type LeaderboardEntry,
	} from '$lib/api';
	import {
		generateKeyPair, loadIdentity, saveIdentity, clearIdentity,
		formatKeyJson, parseKeyFile, canonicalPayload, sign,
		type KeyFile,
	} from '$lib/crypto';
	import { randomGraph, graph6VertexCount } from '$lib/graph6-utils';

	// ── Input state ──────────────────────────────────────────
	let graph6Input = $state('');
	let randomN = $state(25);

	// ── Score state ──────────────────────────────────────────
	let scoring = $state(false);
	let scoreError = $state('');
	let scoreResult = $state<VerifyResponse | null>(null);

	// ── Leaderboard lookup ───────────────────────────────────
	let leaderboardEntry = $state<LeaderboardEntry | null>(null);
	let existingSubmission = $state<{ key_id: string; created_at: string } | null>(null);

	// ── Identity state ───────────────────────────────────────
	let identity = $state<KeyFile | null>(null);
	let keyJsonInput = $state('');
	let keyError = $state('');
	let keyRegistered = $state(false);
	let showKeyJson = $state(false);

	// ── Submit state ─────────────────────────────────────────
	let submitting = $state(false);
	let submitError = $state('');
	let submitResult = $state<SubmitResponse | null>(null);

	// ── Derived ──────────────────────────────────────────────
	const detectedN = $derived(graph6VertexCount(graph6Input.trim()));
	const matrix = $derived.by(() => {
		try {
			const g6 = graph6Input.trim();
			if (!g6) return null;
			const m = decodeGraph6(g6);
			return m.length > 0 ? m : null;
		} catch { return null; }
	});
	const edges = $derived(matrix ? edgeCount(matrix) : 0);
	const totalPossibleEdges = $derived(detectedN ? (detectedN * (detectedN - 1)) / 2 : 0);
	const edgeDensity = $derived(totalPossibleEdges > 0 ? (edges / totalPossibleEdges * 100).toFixed(1) : '0');

	// ── Load saved identity on mount ─────────────────────────
	$effect(() => {
		const saved = loadIdentity();
		if (saved) {
			identity = saved;
			keyRegistered = true; // assume registered if saved
		}
	});

	// ── Score the graph ──────────────────────────────────────
	async function doScore() {
		const g6 = graph6Input.trim();
		if (!g6) return;
		const n = graph6VertexCount(g6);
		if (!n || n === 0) {
			scoreError = 'Cannot determine vertex count from graph6';
			return;
		}

		scoring = true;
		scoreError = '';
		scoreResult = null;
		leaderboardEntry = null;
		existingSubmission = null;
		submitResult = null;
		submitError = '';

		try {
			const result = await verifyGraph(n, g6);
			scoreResult = result;

			// Check if this CID already exists on the leaderboard
			await checkLeaderboard(result.cid, n);
		} catch (e: any) {
			scoreError = e.message || 'Scoring failed';
		} finally {
			scoring = false;
		}
	}

	async function checkLeaderboard(cid: string, n: number) {
		// Check if already submitted
		try {
			const detail = await getSubmission(cid);
			if (detail.submission) {
				existingSubmission = {
					key_id: detail.submission.key_id,
					created_at: detail.submission.created_at,
				};
			}
		} catch { /* not submitted yet, fine */ }

		// Check leaderboard position
		try {
			const lb = await getLeaderboard(n, 500, 0);
			const entry = lb.entries.find(e => e.cid === cid);
			if (entry) {
				leaderboardEntry = entry;
			}
		} catch { /* leaderboard may not exist yet */ }
	}

	// ── Generate random graph ────────────────────────────────
	function doRandom() {
		graph6Input = randomGraph(randomN);
		// Auto-score
		doScore();
	}

	// ── Identity management ──────────────────────────────────
	function doGenerateKey() {
		const kf = generateKeyPair('web-user');
		identity = kf;
		saveIdentity(kf);
		keyRegistered = false;
		keyError = '';
		showKeyJson = true;
	}

	function doLoadKey() {
		keyError = '';
		try {
			const kf = parseKeyFile(keyJsonInput);
			identity = kf;
			saveIdentity(kf);
			keyRegistered = false;
			keyJsonInput = '';
		} catch (e: any) {
			keyError = e.message || 'Invalid key.json';
		}
	}

	function doClearKey() {
		identity = null;
		clearIdentity();
		keyRegistered = false;
		keyError = '';
		showKeyJson = false;
	}

	async function doRegisterKey() {
		if (!identity) return;
		keyError = '';
		try {
			await registerKey(identity.public_key, identity.display_name);
			keyRegistered = true;
			saveIdentity(identity);
		} catch (e: any) {
			// "duplicate key" means already registered, that's fine
			if (e.message?.includes('duplicate') || e.message?.includes('already')) {
				keyRegistered = true;
			} else {
				keyError = e.message || 'Registration failed';
			}
		}
	}

	// ── Submit to leaderboard ────────────────────────────────
	async function doSubmit() {
		if (!scoreResult || !identity) return;

		submitting = true;
		submitError = '';
		submitResult = null;

		try {
			// Register key if not yet registered
			if (!keyRegistered) {
				await doRegisterKey();
				if (!keyRegistered) {
					submitting = false;
					return;
				}
			}

			const g6 = graph6Input.trim();
			const n = scoreResult.n;
			const payload = canonicalPayload(n, g6);
			const sig = sign(payload, identity.secret_key, identity.public_key);
			const result = await submitGraph(n, g6, identity.key_id, sig, {
				source: 'web-submit',
			});
			submitResult = result;

			// Re-check leaderboard
			await checkLeaderboard(result.cid, n);
		} catch (e: any) {
			submitError = e.message || 'Submission failed';
		} finally {
			submitting = false;
		}
	}

	function copyToClipboard(text: string) {
		navigator.clipboard.writeText(text);
	}
</script>

<div class="page">
	<h1>Submit & Explore</h1>
	<p class="subtitle">Score graphs, explore Ramsey properties, and submit to the leaderboard.</p>

	<!-- ── Input Section ──────────────────────────────────── -->
	<div class="card input-section">
		<h3>Graph Input</h3>
		<div class="input-row">
			<textarea
				class="mono graph-input"
				placeholder="Paste a graph6 string..."
				bind:value={graph6Input}
				rows="2"
			></textarea>
		</div>

		<div class="input-controls">
			<div class="input-info">
				{#if detectedN}
					<span class="badge badge-info">n = {detectedN}</span>
					{#if matrix}
						<span class="badge badge-info">{edges} edges ({edgeDensity}%)</span>
					{/if}
				{:else if graph6Input.trim()}
					<span class="badge badge-red">invalid</span>
				{/if}
			</div>

			<div class="input-actions">
				<div class="random-group">
					<label class="random-label">
						n=<input type="number" class="n-input mono" bind:value={randomN} min={2} max={62} />
					</label>
					<button class="btn btn-secondary" onclick={doRandom}>Random</button>
				</div>
				<button
					class="btn btn-primary"
					onclick={doScore}
					disabled={scoring || !graph6Input.trim()}
				>
					{scoring ? 'Scoring...' : 'Score'}
				</button>
			</div>
		</div>
	</div>

	<!-- ── Score Error ────────────────────────────────────── -->
	{#if scoreError}
		<div class="card error-card">
			<p class="error">{scoreError}</p>
		</div>
	{/if}

	<!-- ── Scoring Shimmer ────────────────────────────────── -->
	{#if scoring}
		<div class="shimmer" style="height: 300px; border-radius: 0.75rem;"></div>
	{/if}

	<!-- ── Score Results ──────────────────────────────────── -->
	{#if scoreResult}
		<div class="results-grid">
			<!-- Gem Visualization -->
			<div class="viz-col">
				<GemView
					graph6={scoreResult.canonical_graph6}
					n={scoreResult.n}
					size={320}
					cid={scoreResult.cid}
					goodmanGap={scoreResult.goodman_gap}
					autOrder={scoreResult.aut_order}
					histogram={scoreResult.histogram.tiers}
					label={scoreResult.cid.slice(0, 24)}
				/>
			</div>

			<!-- Score Details -->
			<div class="info-col">
				<div class="card">
					<h3>Graph Properties</h3>
					<dl>
						<dt>CID</dt>
						<dd>
							<span class="mono cid-text">{scoreResult.cid}</span>
							<button class="copy-btn" onclick={() => copyToClipboard(scoreResult!.cid)} title="Copy CID">
								<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
							</button>
						</dd>
						<dt>Vertices</dt>
						<dd>{scoreResult.n}</dd>
						<dt>Edges</dt>
						<dd>{edges} / {totalPossibleEdges} ({edgeDensity}%)</dd>
						<dt>Canonical graph6</dt>
						<dd class="mono g6-text">
							{scoreResult.canonical_graph6}
							<button class="copy-btn" onclick={() => copyToClipboard(scoreResult!.canonical_graph6)} title="Copy graph6">
								<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
							</button>
						</dd>
					</dl>
				</div>

				<div class="card">
					<h3>Score</h3>
					<dl>
						<dt>Goodman Gap</dt>
						<dd>
							<span class="score-value">{scoreResult.goodman_gap}</span>
							{#if scoreResult.goodman_gap === 0}
								<span class="badge badge-green" style="margin-left: 0.5rem;">optimal</span>
							{/if}
						</dd>
						<dt>|Aut(G)|</dt>
						<dd><span class="score-value">{scoreResult.aut_order}</span></dd>
						<dt>Histogram</dt>
						<dd class="histogram">
							{#each scoreResult.histogram.tiers as tier}
								<div class="tier mono">
									<span class="tier-k">k={tier.k}</span>
									<span class="tier-red">red={tier.red}</span>
									<span class="tier-blue">blue={tier.blue}</span>
									{#if tier.red === 0 && tier.blue === 0}
										<span class="badge badge-green" style="margin-left: 0.3rem; font-size: 0.6rem;">Ramsey</span>
									{/if}
								</div>
							{/each}
						</dd>
					</dl>
				</div>

				<!-- Leaderboard Status -->
				{#if leaderboardEntry}
					<div class="card">
						<h3>Leaderboard</h3>
						<dl>
							<dt>Status</dt>
							<dd><span class="badge badge-green">on leaderboard</span></dd>
							<dt>Rank</dt>
							<dd>
								<a href="/leaderboards/{scoreResult.n}">
									#{leaderboardEntry.rank}
								</a>
							</dd>
						</dl>
					</div>
				{/if}

				{#if existingSubmission}
					<div class="card">
						<h3>Prior Submission</h3>
						<dl>
							<dt>Submitter</dt>
							<dd class="mono">
								<a href="/identities/{existingSubmission.key_id}">{existingSubmission.key_id}</a>
							</dd>
							<dt>Submitted</dt>
							<dd>{new Date(existingSubmission.created_at).toLocaleString()}</dd>
							<dt>Detail</dt>
							<dd><a href="/submissions/{scoreResult.cid}">View full submission</a></dd>
						</dl>
					</div>
				{/if}
			</div>
		</div>

		<!-- ── Submit Section ──────────────────────────────── -->
		<div class="card submit-section">
			<h3>Submit to Leaderboard</h3>

			<!-- Identity -->
			<div class="identity-area">
				{#if identity}
					<div class="identity-info">
						<span class="identity-label">Key:</span>
						<span class="mono">{identity.key_id}</span>
						{#if keyRegistered}
							<span class="badge badge-green">registered</span>
						{:else}
							<span class="badge badge-amber">not registered</span>
						{/if}
						<button class="btn-link" onclick={() => { showKeyJson = !showKeyJson; }}>
							{showKeyJson ? 'hide' : 'show'} key.json
						</button>
						<button class="btn-link danger" onclick={doClearKey}>clear</button>
					</div>

					{#if showKeyJson}
						<div class="key-json-display">
							<pre class="mono">{formatKeyJson(identity)}</pre>
							<div class="key-json-actions">
								<button class="btn btn-small" onclick={() => copyToClipboard(formatKeyJson(identity!))}>
									Copy key.json
								</button>
								<p class="key-warning">Save this key! You need it to prove ownership of submissions.</p>
							</div>
						</div>
					{/if}
				{:else}
					<div class="identity-setup">
						<button class="btn btn-secondary" onclick={doGenerateKey}>
							Generate New Key
						</button>
						<span class="or-divider">or</span>
						<div class="paste-key">
							<textarea
								class="mono key-input"
								placeholder='Paste key.json contents...'
								bind:value={keyJsonInput}
								rows="3"
							></textarea>
							<button class="btn btn-secondary" onclick={doLoadKey} disabled={!keyJsonInput.trim()}>
								Load Key
							</button>
						</div>
					</div>
				{/if}
				{#if keyError}
					<p class="error" style="margin-top: 0.5rem;">{keyError}</p>
				{/if}
			</div>

			<!-- Submit button -->
			<div class="submit-actions">
				<button
					class="btn btn-primary btn-large"
					onclick={doSubmit}
					disabled={submitting || !identity}
				>
					{#if submitting}
						Submitting...
					{:else}
						Submit to Leaderboard
					{/if}
				</button>
			</div>

			{#if submitError}
				<p class="error" style="margin-top: 0.75rem;">{submitError}</p>
			{/if}

			{#if submitResult}
				<div class="submit-result">
					<div class="verdict">
						{#if submitResult.admitted}
							<span class="badge badge-green">Admitted at rank #{submitResult.rank}</span>
						{:else}
							<span class="badge badge-amber">Accepted (not admitted to leaderboard)</span>
						{/if}
					</div>
					<dl>
						<dt>Verdict</dt>
						<dd>{submitResult.verdict}</dd>
						<dt>CID</dt>
						<dd class="mono"><a href="/submissions/{submitResult.cid}">{submitResult.cid.slice(0, 32)}...</a></dd>
						{#if submitResult.admitted && submitResult.rank}
							<dt>Leaderboard</dt>
							<dd><a href="/leaderboards/{scoreResult.n}">View leaderboard (n={scoreResult.n})</a></dd>
						{/if}
					</dl>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.page {
		max-width: 1000px;
		width: 100%;
	}

	h1 {
		font-family: var(--font-mono);
		font-size: 1.5rem;
		margin-bottom: 0.25rem;
	}

	.subtitle {
		color: var(--color-text-muted);
		font-size: 0.85rem;
		margin-bottom: 1.5rem;
	}

	h3 {
		font-size: 0.8rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-text-muted);
		margin-bottom: 0.75rem;
	}

	/* Input section */
	.input-section { margin-bottom: 1.5rem; }

	.graph-input {
		width: 100%;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		padding: 0.75rem;
		color: var(--color-text);
		font-size: 0.85rem;
		resize: vertical;
		line-height: 1.5;
	}
	.graph-input:focus {
		outline: none;
		border-color: var(--color-accent);
	}

	.input-controls {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-top: 0.75rem;
		flex-wrap: wrap;
		gap: 0.5rem;
	}

	.input-info { display: flex; gap: 0.5rem; align-items: center; }

	.input-actions { display: flex; gap: 0.75rem; align-items: center; }

	.random-group { display: flex; gap: 0.4rem; align-items: center; }

	.random-label {
		font-size: 0.8rem;
		font-family: var(--font-mono);
		color: var(--color-text-muted);
		display: flex;
		align-items: center;
		gap: 0.2rem;
	}

	.n-input {
		width: 3.5rem;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.3rem;
		padding: 0.25rem 0.4rem;
		color: var(--color-text);
		font-size: 0.8rem;
		text-align: center;
	}
	.n-input:focus { outline: none; border-color: var(--color-accent); }

	/* Badges */
	.badge-info {
		background: rgba(99, 102, 241, 0.12);
		color: var(--color-accent);
		border: 1px solid rgba(99, 102, 241, 0.25);
	}

	/* Buttons */
	.btn {
		font-family: var(--font-mono);
		font-size: 0.8rem;
		padding: 0.4rem 0.8rem;
		border-radius: 0.4rem;
		border: 1px solid var(--color-border);
		cursor: pointer;
		transition: all 0.15s;
	}
	.btn:disabled { opacity: 0.4; cursor: not-allowed; }
	.btn-primary {
		background: var(--color-accent);
		color: white;
		border-color: var(--color-accent);
	}
	.btn-primary:hover:not(:disabled) { background: var(--color-accent-hover); }
	.btn-secondary {
		background: var(--color-surface-2);
		color: var(--color-text);
	}
	.btn-secondary:hover:not(:disabled) { border-color: var(--color-accent); }
	.btn-large { padding: 0.6rem 1.5rem; font-size: 0.9rem; }
	.btn-small { font-size: 0.7rem; padding: 0.25rem 0.5rem; }
	.btn-link {
		background: none;
		border: none;
		color: var(--color-accent);
		font-size: 0.75rem;
		cursor: pointer;
		font-family: var(--font-mono);
		padding: 0;
	}
	.btn-link:hover { color: var(--color-accent-hover); }
	.btn-link.danger { color: var(--color-red); }
	.btn-link.danger:hover { color: #f87171; }

	/* Results grid */
	.results-grid {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 1.5rem;
		align-items: start;
		margin-bottom: 1.5rem;
	}
	@media (max-width: 768px) {
		.results-grid { grid-template-columns: 1fr; }
		.viz-col { display: flex; justify-content: center; }
	}

	.info-col { display: flex; flex-direction: column; gap: 1rem; }

	dl { display: grid; grid-template-columns: auto 1fr; gap: 0.3rem 1rem; align-items: baseline; }
	dt { font-size: 0.8rem; color: var(--color-text-muted); white-space: nowrap; }
	dd { font-size: 0.85rem; }

	.cid-text, .g6-text {
		word-break: break-all;
		font-size: 0.7rem;
		display: inline;
	}

	.copy-btn {
		background: none;
		border: none;
		color: var(--color-text-dim);
		cursor: pointer;
		padding: 0.1rem;
		vertical-align: middle;
		margin-left: 0.3rem;
	}
	.copy-btn:hover { color: var(--color-accent); }

	.score-value { font-weight: 600; }

	.histogram { display: flex; flex-direction: column; gap: 0.15rem; }
	.tier { font-size: 0.75rem; display: flex; gap: 0.75rem; align-items: center; }
	.tier-k { color: var(--color-text-muted); min-width: 2.5rem; }
	.tier-red { color: var(--color-red); min-width: 5rem; }
	.tier-blue { color: #60a5fa; min-width: 5rem; }

	/* Error */
	.error { color: var(--color-red); font-size: 0.85rem; }
	.error-card { margin-bottom: 1rem; }

	/* Submit section */
	.submit-section { margin-bottom: 1.5rem; }

	.identity-area { margin-bottom: 1rem; }

	.identity-info {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
		font-size: 0.85rem;
	}
	.identity-label { color: var(--color-text-muted); font-size: 0.8rem; }

	.identity-setup {
		display: flex;
		align-items: flex-start;
		gap: 1rem;
		flex-wrap: wrap;
	}

	.or-divider {
		color: var(--color-text-dim);
		font-size: 0.8rem;
		padding-top: 0.4rem;
	}

	.paste-key {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		flex: 1;
		min-width: 250px;
	}

	.key-input {
		width: 100%;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.4rem;
		padding: 0.5rem;
		color: var(--color-text);
		font-size: 0.7rem;
		resize: vertical;
	}
	.key-input:focus { outline: none; border-color: var(--color-accent); }

	.key-json-display {
		margin-top: 0.75rem;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		padding: 0.75rem;
	}
	.key-json-display pre {
		font-size: 0.7rem;
		white-space: pre-wrap;
		word-break: break-all;
		line-height: 1.5;
	}
	.key-json-actions {
		display: flex;
		align-items: center;
		gap: 1rem;
		margin-top: 0.5rem;
	}
	.key-warning {
		color: var(--color-amber);
		font-size: 0.7rem;
	}

	.submit-actions { margin-top: 0.75rem; }

	.submit-result {
		margin-top: 1rem;
		padding: 1rem;
		background: var(--color-bg);
		border-radius: 0.5rem;
		border: 1px solid var(--color-border);
	}
	.verdict { margin-bottom: 0.75rem; }
</style>
