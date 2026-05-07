type HandlerResult = void | Promise<void>;
type Handler = (params: Record<string, string>) => HandlerResult;

const SWIPE_NAV_MIN_DISTANCE = 72;
const SWIPE_NAV_MAX_VERTICAL_DISTANCE = 80;
const SWIPE_NAV_DIRECTION_RATIO = 1.35;
const SWIPE_NAV_EDGE_GUTTER = 18;

type MatchResult = {
	pattern: string;
	handler: Handler;
	params: Record<string, string>;
} | null;

type SwipeNavigationOptions = {
	paths: string[];
	root?: HTMLElement | Document;
};

let matcher: ReturnType<typeof patternMatcher> | null = null;

export function patternMatcher(handlers: Record<string, Handler>) {
	const routes = Object.keys(handlers).sort((a, b) => {
		if (!a.includes("*") && !a.includes(":")) return -1;
		if (!b.includes("*") && !b.includes(":")) return 1;

		if (a.includes(":") && !b.includes(":")) return -1;
		if (!a.includes(":") && b.includes(":")) return 1;

		if (a.includes("*") && !b.includes("*")) return 1;
		if (!a.includes("*") && b.includes("*")) return -1;

		return b.length - a.length;
	});

	return {
		match(path: string): MatchResult {
			for (const route of routes) {
				const params = matchRoute(route, path);
				if (params !== null) {
					const handler = handlers[route];
					if (!handler) {
						continue;
					}
					return {
						pattern: route,
						handler,
						params,
					};
				}
			}
			return null;
		},
	};
}

function matchRoute(
	pattern: string,
	path: string,
): Record<string, string> | null {
	const patternParts = pattern
		.split("/")
		.filter((segment) => segment.length > 0);
	const pathParts = path.split("/").filter((segment) => segment.length > 0);

	if (pattern === "/*") {
		return {};
	}

	if (patternParts.length !== pathParts.length) {
		const lastPattern = patternParts[patternParts.length - 1] ?? "";
		if (
			lastPattern === "*" &&
			pathParts.length >= patternParts.length - 1
		) {
			return {};
		}
		return null;
	}

	const params: Record<string, string> = {};

	for (let index = 0; index < patternParts.length; index += 1) {
		const patternPart = patternParts[index]!;
		const pathPart = pathParts[index]!;

		if (patternPart === "*") {
			return params;
		}
		if (patternPart.startsWith(":")) {
			params[patternPart.slice(1)] = pathPart;
			continue;
		}
		if (patternPart !== pathPart) {
			return null;
		}
	}

	return params;
}

const handleRoute = async (path: string) => {
	if (!matcher) {
		return;
	}
	const match = matcher.match(path);
	if (!match) {
		console.error("No route found for", path);
		return;
	}
	await Promise.resolve(match.handler(match.params) as HandlerResult);
};

window.addEventListener("popstate", () => {
	void handleRoute(window.location.pathname);
});

export const routes = (handlers: Record<string, Handler>) => {
	matcher = patternMatcher(handlers);
	void handleRoute(window.location.pathname);
};

export const installLinkInterceptor = (root: ParentNode = document) => {
	root.addEventListener("click", (event) => {
		const target = event.target;
		if (!(target instanceof Element)) {
			return;
		}
		const link = target.closest("a[data-link]");
		if (!(link instanceof HTMLAnchorElement)) {
			return;
		}
		const href = link.getAttribute("href");
		if (!href || href.startsWith("http")) {
			return;
		}
		event.preventDefault();
		navigate(href);
	});
};

const shouldIgnoreSwipeTarget = (target: EventTarget | null) =>
	target instanceof Element &&
	target.closest(
		'a, button, input, textarea, select, label, [contenteditable="true"], [data-swipe-nav-ignore], .navbar',
	);

const getCurrentSwipePathIndex = (paths: string[]) =>
	paths.findIndex((path) =>
		path === "/"
			? window.location.pathname === path
			: window.location.pathname === path ||
				window.location.pathname.startsWith(`${path}/`),
	);

const getSwipeNavigationPath = (paths: string[], deltaX: number) => {
	const currentIndex = getCurrentSwipePathIndex(paths);
	if (currentIndex === -1) {
		return null;
	}

	const nextIndex = deltaX < 0 ? currentIndex + 1 : currentIndex - 1;
	return paths[nextIndex] ?? null;
};

const isTouchNavigationAvailable = () =>
	window.matchMedia("(hover: none) and (pointer: coarse)").matches ||
	navigator.maxTouchPoints > 0;

export const installSwipeNavigation = ({
	paths,
	root = document,
}: SwipeNavigationOptions) => {
	let trackingPointerId: number | null = null;
	let startX = 0;
	let startY = 0;

	root.addEventListener("pointerdown", (event) => {
		if (!(event instanceof PointerEvent)) {
			return;
		}
		if (
			!isTouchNavigationAvailable() ||
			event.pointerType !== "touch" ||
			!event.isPrimary ||
			shouldIgnoreSwipeTarget(event.target)
		) {
			return;
		}
		if (
			event.clientX <= SWIPE_NAV_EDGE_GUTTER ||
			event.clientX >= window.innerWidth - SWIPE_NAV_EDGE_GUTTER
		) {
			return;
		}

		trackingPointerId = event.pointerId;
		startX = event.clientX;
		startY = event.clientY;
	});

	root.addEventListener("pointerup", (event) => {
		if (
			!(event instanceof PointerEvent) ||
			trackingPointerId !== event.pointerId
		) {
			return;
		}

		trackingPointerId = null;

		const deltaX = event.clientX - startX;
		const deltaY = event.clientY - startY;
		const absX = Math.abs(deltaX);
		const absY = Math.abs(deltaY);

		if (
			absX < SWIPE_NAV_MIN_DISTANCE ||
			absY > SWIPE_NAV_MAX_VERTICAL_DISTANCE ||
			absX < absY * SWIPE_NAV_DIRECTION_RATIO
		) {
			return;
		}

		const nextPath = getSwipeNavigationPath(paths, deltaX);
		if (!nextPath) {
			return;
		}

		navigate(nextPath);
	});

	root.addEventListener("pointercancel", (event) => {
		if (
			event instanceof PointerEvent &&
			trackingPointerId === event.pointerId
		) {
			trackingPointerId = null;
		}
	});
};

export const navigate = (path: string) => {
	if (window.location.pathname !== path) {
		window.history.pushState({}, "", path);
	}
	void handleRoute(path);
};
