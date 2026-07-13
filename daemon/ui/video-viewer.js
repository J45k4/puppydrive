export default class VideoViewer {
  constructor(element) {
    this.element = element;
  }

  mount(props) {
    this.setProps(props);
  }

  setProps(props) {
    const video = document.createElement("video");
    video.src = String(props.src);
    video.controls = true;
    video.autoplay = true;
    video.preload = "metadata";
    video.style.width = "100%";
    video.style.height = "100%";
    video.style.display = "block";
    video.style.background = "#111827";
    video.style.objectFit = "contain";
    this.element.replaceChildren(video);
  }

  dispose() {
    this.element.replaceChildren();
  }
}
