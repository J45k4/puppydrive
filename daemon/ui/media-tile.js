export default class MediaTile {
  constructor(element, ctx) {
    this.element = element;
    this.ctx = ctx;
    this.image = null;
    this.observer = null;
  }

  mount(props) {
    this.setProps(props);
  }

  setProps(props) {
    this.clear();
    const thumbnailSize = Math.min(
      320,
      Math.max(140, Number(props.thumbnailSize) || 220),
    );
    const previewHeight = Math.round(thumbnailSize * 0.75);
    const root = document.createElement("button");
    root.type = "button";
    root.style.display = "flex";
    root.style.flexDirection = "column";
    root.style.width = "100%";
    root.style.height = "100%";
    root.style.padding = "0";
    root.style.overflow = "hidden";
    root.style.border = "1px solid #dce5e8";
    root.style.borderRadius = "6px";
    root.style.background = "#ffffff";
    root.style.color = "#1f2937";
    root.style.textAlign = "left";
    root.style.cursor = "pointer";
    root.onclick = () =>
      this.ctx.emit("open", { index: Number(props.index) });

    const preview = document.createElement("div");
    preview.style.display = "flex";
    preview.style.alignItems = "center";
    preview.style.justifyContent = "center";
    preview.style.width = "100%";
    preview.style.height = `${previewHeight}px`;
    preview.style.flex = `0 0 ${previewHeight}px`;
    preview.style.overflow = "hidden";
    preview.style.background = "#111827";

    if (props.kind === "image") {
      const image = document.createElement("img");
      image.alt = String(props.name ?? "");
      image.draggable = false;
      image.style.width = "100%";
      image.style.height = "100%";
      image.style.objectFit = "cover";
      image.style.pointerEvents = "none";
      preview.append(image);
      this.image = image;
      const src = String(props.src ?? "");
      this.observer = new IntersectionObserver(
        (entries) => {
          for (const entry of entries) {
            if (!entry.isIntersecting || !src) continue;
            image.src = src;
            this.observer?.unobserve(preview);
          }
        },
        { root: null, rootMargin: "0px", threshold: 0.01 },
      );
      this.observer.observe(preview);
    } else {
      const icon = document.createElement("span");
      icon.textContent = "▶";
      icon.style.color = "#e5f4f7";
      icon.style.fontSize = "34px";
      icon.style.transform = "translateX(2px)";
      preview.append(icon);
    }

    const caption = document.createElement("div");
    caption.style.width = "100%";
    caption.style.padding = "8px 10px";
    caption.style.boxSizing = "border-box";

    const name = document.createElement("div");
    name.textContent = String(props.name ?? "");
    name.title = String(props.name ?? "");
    name.style.overflow = "hidden";
    name.style.textOverflow = "ellipsis";
    name.style.whiteSpace = "nowrap";

    const details = document.createElement("div");
    details.textContent = `${String(props.size ?? "")}  •  ${String(props.modified ?? "")}`;
    details.style.marginTop = "3px";
    details.style.overflow = "hidden";
    details.style.color = "#6b7280";
    details.style.fontSize = "12px";
    details.style.textOverflow = "ellipsis";
    details.style.whiteSpace = "nowrap";

    caption.append(name, details);
    root.append(preview, caption);
    this.element.replaceChildren(root);
  }

  dispose() {
    this.clear();
    this.element.replaceChildren();
  }

  clear() {
    this.observer?.disconnect();
    this.observer = null;
    if (this.image) {
      this.image.removeAttribute("src");
      this.image.src = "";
      this.image = null;
    }
  }
}
