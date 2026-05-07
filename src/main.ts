import index from "./web/index.html";
import { mkdir, readdir, stat } from "node:fs/promises";
import { basename, join } from "node:path";

const dataDir = join(import.meta.dir, "..", "data");
const port = Number(Bun.env.PORT ?? 3334);

type StoredFile = {
	name: string;
	size: number;
	updatedAt: string;
	url: string;
};

const ensureDataDir = () => mkdir(dataDir, { recursive: true });

const sanitizeFileName = (name: string) => {
	const fallback = "upload";
	const normalized = basename(name).replace(/[^\w .()[\]-]/g, "_").trim();
	return normalized.length > 0 ? normalized : fallback;
};

const uniqueFilePath = async (fileName: string) => {
	const extensionIndex = fileName.lastIndexOf(".");
	const base =
		extensionIndex > 0 ? fileName.slice(0, extensionIndex) : fileName;
	const extension = extensionIndex > 0 ? fileName.slice(extensionIndex) : "";
	let candidate = fileName;
	let counter = 1;

	while (await Bun.file(join(dataDir, candidate)).exists()) {
		candidate = `${base}-${counter}${extension}`;
		counter += 1;
	}

	return {
		name: candidate,
		path: join(dataDir, candidate),
	};
};

const json = (body: unknown, init?: ResponseInit) =>
	Response.json(body, {
		...init,
		headers: {
			"Cache-Control": "no-store",
			...init?.headers,
		},
	});

const listFiles = async (): Promise<StoredFile[]> => {
	await ensureDataDir();
	const entries = await readdir(dataDir);
	const files = await Promise.all(
		entries.map(async (name) => {
			const filePath = join(dataDir, name);
			const fileStat = await stat(filePath);
			if (!fileStat.isFile()) {
				return null;
			}

			return {
				name,
				size: fileStat.size,
				updatedAt: fileStat.mtime.toISOString(),
				url: `/files/${encodeURIComponent(name)}`,
			};
		}),
	);

	return files
		.filter((file): file is StoredFile => file !== null)
		.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
};

const handleUpload = async (request: Request) => {
	await ensureDataDir();
	const form = await request.formData();
	const uploads = form.getAll("files").filter((value): value is File => {
		return value instanceof File && value.size > 0;
	});

	if (uploads.length === 0) {
		return json({ error: "Choose at least one file to upload." }, { status: 400 });
	}

	const saved = [];
	for (const upload of uploads) {
		const safeName = sanitizeFileName(upload.name);
		const target = await uniqueFilePath(safeName);
		await Bun.write(target.path, upload);
		saved.push({
			name: target.name,
			size: upload.size,
			url: `/files/${encodeURIComponent(target.name)}`,
		});
	}

	return json({ files: saved }, { status: 201 });
};

const serveStoredFile = async (name: string) => {
	const safeName = sanitizeFileName(name);
	if (safeName !== name || safeName === "." || safeName === "..") {
		return new Response("Not found", { status: 404 });
	}

	const file = Bun.file(join(dataDir, safeName));
	if (!(await file.exists())) {
		return new Response("Not found", { status: 404 });
	}

	return new Response(file, {
		headers: {
			"Content-Disposition": `attachment; filename="${safeName.replaceAll('"', "_")}"`,
		},
	});
};

const server = Bun.serve({
	port,
	routes: {
		"/": index,
		"/api/files": {
			GET: async () => json({ files: await listFiles() }),
			POST: handleUpload,
		},
		"/api/*": () => json({ error: "Not found" }, { status: 404 }),
		"/files/:name": (request) => serveStoredFile(request.params.name),
		"/*": index,
	},
	development: {
		hmr: true,
		console: true,
	},
});

console.log(`puppydrive listening on ${server.url}`);
