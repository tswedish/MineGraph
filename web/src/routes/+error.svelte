<script lang="ts">
	import { page } from '$app/state';

	// Pick a fun graph-theory quip based on status code
	const quips: Record<number, string> = {
		404: 'This edge leads nowhere — the vertex you seek is not in our graph.',
		500: 'An unexpected clique has formed in our server internals.',
		503: 'The search space is temporarily exhausted. Try again shortly.',
	};

	const quip = $derived(quips[page.status] ?? 'A cycle has been detected in an unexpected place.');

	// SVG: a small "broken graph" — 5 vertices with a dangling edge leading to nowhere
	// Vertices arranged in a pentagon, with vertex 4 detached and its edge dashed
</script>

<div class="error-page">
	<div class="error-viz">
		<svg viewBox="0 0 200 200" width="200" height="200" aria-label="Broken graph illustration">
			<!-- Edges of the connected component -->
			<line x1="100" y1="30" x2="165" y2="85" stroke="var(--color-border)" stroke-width="2"/>
			<line x1="165" y1="85" x2="140" y2="160" stroke="var(--color-border)" stroke-width="2"/>
			<line x1="140" y1="160" x2="60" y2="160" stroke="var(--color-border)" stroke-width="2"/>
			<line x1="60" y1="160" x2="35" y2="85" stroke="var(--color-border)" stroke-width="2"/>
			<line x1="35" y1="85" x2="100" y2="30" stroke="var(--color-border)" stroke-width="2"/>

			<!-- The broken edge — dashed, leading off to a missing vertex -->
			<line x1="165" y1="85" x2="198" y2="38" stroke="var(--color-rejected)" stroke-width="2" stroke-dasharray="6 4" opacity="0.7"/>
			<circle cx="198" cy="38" r="5" fill="none" stroke="var(--color-rejected)" stroke-width="1.5" stroke-dasharray="3 3" opacity="0.5"/>

			<!-- Vertices -->
			<circle cx="100" cy="30" r="6" fill="var(--color-accent)"/>
			<circle cx="165" cy="85" r="6" fill="var(--color-accent)"/>
			<circle cx="140" cy="160" r="6" fill="var(--color-accent)"/>
			<circle cx="60" cy="160" r="6" fill="var(--color-accent)"/>
			<circle cx="35" cy="85" r="6" fill="var(--color-accent)"/>

			<!-- Question mark near the missing vertex -->
			<text x="192" y="22" fill="var(--color-rejected)" font-family="var(--font-mono)" font-size="14" text-anchor="middle" opacity="0.7">?</text>
		</svg>
	</div>

	<h1 class="error-code">{page.status}</h1>
	<p class="error-quip">{quip}</p>
	<p class="error-detail">{page.error?.message ?? 'Something went wrong.'}</p>

	<a href="/" class="home-link">Back to R(home, home)</a>
</div>

<style>
	.error-page {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		text-align: center;
		padding: 3rem 1.5rem;
		min-height: 60vh;
		gap: 1rem;
	}

	.error-viz {
		margin-bottom: 0.5rem;
		opacity: 0.9;
	}

	.error-code {
		font-family: var(--font-mono);
		font-size: 4rem;
		font-weight: 800;
		color: var(--color-text-muted);
		line-height: 1;
		letter-spacing: -0.04em;
	}

	.error-quip {
		font-family: var(--font-mono);
		font-size: 0.9375rem;
		color: var(--color-text);
		max-width: 480px;
		line-height: 1.5;
	}

	.error-detail {
		font-size: 0.8125rem;
		color: var(--color-text-muted);
		max-width: 400px;
	}

	.home-link {
		margin-top: 1.5rem;
		font-family: var(--font-mono);
		font-size: 0.875rem;
		color: var(--color-accent);
		padding: 0.5rem 1.25rem;
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		transition: border-color 0.2s;
	}

	.home-link:hover {
		border-color: var(--color-accent);
		color: var(--color-accent-hover);
	}
</style>
