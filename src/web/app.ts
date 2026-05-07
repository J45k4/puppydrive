import { installLinkInterceptor, routes } from "./router";

type StoredFile = {
	name: string;
	size: number;
	updatedAt: string;
	url: string;
};

const app = document.querySelector<HTMLDivElement>("#app");

if (!app) {
	throw new Error("Missing #app root");
}

const formatBytes = (bytes: number) => {
	if (bytes === 0) {
		return "0 B";
	}

	const units = ["B", "KB", "MB", "GB", "TB"];
	const unitIndex = Math.min(
		Math.floor(Math.log(bytes) / Math.log(1024)),
		units.length - 1,
	);
	const value = bytes / 1024 ** unitIndex;
	return `${value.toFixed(value >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
};

const formatDate = (value: string) =>
	new Intl.DateTimeFormat(undefined, {
		dateStyle: "medium",
		timeStyle: "short",
	}).format(new Date(value));

const escapeHtml = (value: string) =>
	value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");

const loadFiles = async () => {
	const response = await fetch("/api/files");
	if (!response.ok) {
		throw new Error("Could not load files");
	}
	return (await response.json()) as { files: StoredFile[] };
};

const renderFileRows = (files: StoredFile[]) => {
	if (files.length === 0) {
		return `
			<div class="empty-state">
				<strong>No files yet</strong>
				<span>Upload something to store it on this server.</span>
			</div>
		`;
	}

	return `
		<ul class="file-list">
			${files
				.map(
					(file) => `
						<li class="file-row">
							<a href="${file.url}" class="file-name">${escapeHtml(file.name)}</a>
							<span>${formatBytes(file.size)}</span>
							<span>${formatDate(file.updatedAt)}</span>
						</li>
					`,
				)
				.join("")}
		</ul>
	`;
};

const renderHome = async () => {
	app.innerHTML = `
		<main class="shell">
			<section class="upload-panel">
				<div>
					<p class="eyebrow">puppydrive</p>
					<h1>Server file drop</h1>
				</div>
				<form id="upload-form" class="upload-form">
					<label class="drop-zone">
						<input id="file-input" name="files" type="file" multiple />
						<span>Choose files</span>
					</label>
					<button type="submit">Upload</button>
				</form>
				<p id="status" class="status" aria-live="polite"></p>
			</section>
			<section class="files-panel">
				<div class="section-header">
					<h2>Files</h2>
					<button id="refresh" type="button">Refresh</button>
				</div>
				<div id="files">Loading...</div>
			</section>
		</main>
	`;

	const filesRoot = document.querySelector<HTMLDivElement>("#files");
	const status = document.querySelector<HTMLParagraphElement>("#status");
	const form = document.querySelector<HTMLFormElement>("#upload-form");
	const refresh = document.querySelector<HTMLButtonElement>("#refresh");

	const refreshFiles = async () => {
		if (!filesRoot) {
			return;
		}
		filesRoot.textContent = "Loading...";
		try {
			const { files } = await loadFiles();
			filesRoot.innerHTML = renderFileRows(files);
		} catch (error) {
			filesRoot.innerHTML = `<div class="empty-state"><strong>Could not load files</strong><span>${escapeHtml(String(error))}</span></div>`;
		}
	};

	form?.addEventListener("submit", async (event) => {
		event.preventDefault();
		if (!form || !status) {
			return;
		}

		const submitButton = form.querySelector<HTMLButtonElement>("button");
		submitButton?.setAttribute("disabled", "true");
		status.textContent = "Uploading...";

		try {
			const response = await fetch("/api/files", {
				method: "POST",
				body: new FormData(form),
			});
			const body = (await response.json()) as {
				files?: StoredFile[];
				error?: string;
			};

			if (!response.ok) {
				throw new Error(body.error ?? "Upload failed");
			}

			form.reset();
			status.textContent = `Uploaded ${body.files?.length ?? 0} file${body.files?.length === 1 ? "" : "s"}.`;
			await refreshFiles();
		} catch (error) {
			status.textContent = error instanceof Error ? error.message : "Upload failed";
		} finally {
			submitButton?.removeAttribute("disabled");
		}
	});

	refresh?.addEventListener("click", () => {
		void refreshFiles();
	});

	await refreshFiles();
};

installLinkInterceptor(document.body);

routes({
	"/": renderHome,
	"/*": renderHome,
});
