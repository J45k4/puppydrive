export default class ImageViewer {
  constructor(element) {
    this.element = element;
    this.src = null;
    this.scale = 1;
    this.offsetX = 0;
    this.offsetY = 0;
    this.pointerId = null;
  }

  mount(props) {
    this.render(props);
  }

  setProps(props) {
    const src = String(props.src);
    if (this.image && src === this.src) {
      this.image.alt = String(props.alt ?? "");
      return;
    }

    this.clear();
    this.render(props);
  }

  render(props) {
    this.src = String(props.src);
    this.scale = 1;
    this.offsetX = 0;
    this.offsetY = 0;

    const viewport = document.createElement("div");
    viewport.style.position = "relative";
    viewport.style.width = "100%";
    viewport.style.height = "100%";
    viewport.style.overflow = "hidden";
    viewport.style.userSelect = "none";
    viewport.style.touchAction = "none";

    const layer = document.createElement("div");
    layer.style.position = "absolute";
    layer.style.inset = "0";
    layer.style.transformOrigin = "50% 50%";

    const image = document.createElement("img");
    image.src = this.src;
    image.alt = String(props.alt ?? "");
    image.draggable = false;
    image.style.position = "absolute";
    image.style.left = "50%";
    image.style.top = "50%";
    image.style.maxWidth = "100%";
    image.style.maxHeight = "100%";
    image.style.display = "block";
    image.style.pointerEvents = "none";
    image.style.transform = "translate(-50%, -50%)";

    const zoomLabel = document.createElement("div");
    zoomLabel.style.position = "absolute";
    zoomLabel.style.right = "12px";
    zoomLabel.style.top = "12px";
    zoomLabel.style.padding = "4px 7px";
    zoomLabel.style.borderRadius = "4px";
    zoomLabel.style.background = "rgba(17, 24, 39, 0.75)";
    zoomLabel.style.color = "#f9fafb";
    zoomLabel.style.fontSize = "12px";
    zoomLabel.style.pointerEvents = "none";

    this.viewport = viewport;
    this.layer = layer;
    this.image = image;
    this.zoomLabel = zoomLabel;

    this.onWheel = (event) => {
      event.preventDefault();

      const oldScale = this.scale;
      const newScale = Math.min(
        10,
        Math.max(1, oldScale * Math.exp(-event.deltaY * 0.0015)),
      );
      if (newScale === oldScale) return;

      const rect = viewport.getBoundingClientRect();
      const pointerX = event.clientX - rect.left - rect.width / 2;
      const pointerY = event.clientY - rect.top - rect.height / 2;
      const ratio = newScale / oldScale;
      this.offsetX = pointerX - (pointerX - this.offsetX) * ratio;
      this.offsetY = pointerY - (pointerY - this.offsetY) * ratio;
      this.scale = newScale;
      this.applyTransform();
    };

    this.onPointerDown = (event) => {
      if (event.button !== 0 || this.scale <= 1) return;
      event.preventDefault();
      this.pointerId = event.pointerId;
      this.lastPointerX = event.clientX;
      this.lastPointerY = event.clientY;
      viewport.setPointerCapture(event.pointerId);
      viewport.style.cursor = "grabbing";
    };

    this.onPointerMove = (event) => {
      if (event.pointerId !== this.pointerId) return;
      this.offsetX += event.clientX - this.lastPointerX;
      this.offsetY += event.clientY - this.lastPointerY;
      this.lastPointerX = event.clientX;
      this.lastPointerY = event.clientY;
      this.applyTransform();
    };

    this.onPointerUp = (event) => {
      if (event.pointerId !== this.pointerId) return;
      if (viewport.hasPointerCapture(event.pointerId)) {
        viewport.releasePointerCapture(event.pointerId);
      }
      this.pointerId = null;
      viewport.style.cursor = this.scale > 1 ? "grab" : "default";
    };

    viewport.addEventListener("wheel", this.onWheel, { passive: false });
    viewport.addEventListener("pointerdown", this.onPointerDown);
    viewport.addEventListener("pointermove", this.onPointerMove);
    viewport.addEventListener("pointerup", this.onPointerUp);
    viewport.addEventListener("pointercancel", this.onPointerUp);
    image.addEventListener("load", () => this.applyTransform(), { once: true });

    layer.append(image);
    viewport.append(layer, zoomLabel);
    this.element.replaceChildren(viewport);
    this.applyTransform();
  }

  applyTransform() {
    if (!this.viewport || !this.layer || !this.image) return;

    if (this.scale <= 1) {
      this.scale = 1;
      this.offsetX = 0;
      this.offsetY = 0;
    } else {
      const maxX = Math.max(
        0,
        (this.image.offsetWidth * this.scale - this.viewport.clientWidth) / 2,
      );
      const maxY = Math.max(
        0,
        (this.image.offsetHeight * this.scale - this.viewport.clientHeight) / 2,
      );
      this.offsetX = Math.min(maxX, Math.max(-maxX, this.offsetX));
      this.offsetY = Math.min(maxY, Math.max(-maxY, this.offsetY));
    }

    this.layer.style.transform = `translate(${this.offsetX}px, ${this.offsetY}px) scale(${this.scale})`;
    this.viewport.style.cursor =
      this.pointerId !== null ? "grabbing" : this.scale > 1 ? "grab" : "default";
    this.zoomLabel.textContent = `${Math.round(this.scale * 100)}%`;
  }

  clear() {
    if (this.viewport) {
      this.viewport.removeEventListener("wheel", this.onWheel);
      this.viewport.removeEventListener("pointerdown", this.onPointerDown);
      this.viewport.removeEventListener("pointermove", this.onPointerMove);
      this.viewport.removeEventListener("pointerup", this.onPointerUp);
      this.viewport.removeEventListener("pointercancel", this.onPointerUp);
    }
    this.viewport = null;
    this.layer = null;
    this.image = null;
    this.zoomLabel = null;
    this.pointerId = null;
  }

  dispose() {
    this.clear();
    this.element.replaceChildren();
  }
}
