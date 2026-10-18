<script setup lang="ts">
/**
 * three.js preview of a whole skin.
 *
 * Every mesh of the selected skin is rendered together in one scene (bind pose,
 * same coordinate space), so the character is complete instead of one part at a
 * time. Geometry arrives as typed arrays from the Python core.
 */
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { t } from "../lib/i18n";
import type { PreviewGeometry } from "../lib/preview";

const props = defineProps<{
  meshes: PreviewGeometry[];
  hidden: string[];
  selected: string[];
  loading?: boolean;
}>();

const host = ref<HTMLDivElement | null>(null);
const zoomPct = ref(100);

let renderer: THREE.WebGLRenderer | null = null;
let scene: THREE.Scene | null = null;
let camera: THREE.PerspectiveCamera | null = null;
let controls: OrbitControls | null = null;
let observer: ResizeObserver | null = null;
let group: THREE.Group | null = null;
let grid: THREE.GridHelper | null = null;
let raf = 0;
const built = new Map<string, THREE.Mesh>();
/** 相机取景参数，供「重置视角」和缩放百分比复用。 */
let framing: { center: THREE.Vector3; maxDim: number } | null = null;

const COLOR_PICKED = 0x8fc9ff;
const COLOR_IDLE = 0x5f6a78;

function render() {
  if (!renderer || !scene || !camera) return;
  renderer.render(scene, camera);
}

function resize() {
  const el = host.value;
  if (!el || !renderer || !camera) return;
  const width = Math.max(1, el.clientWidth);
  const height = Math.max(1, el.clientHeight);
  renderer.setSize(width, height, false);
  camera.aspect = width / height;
  camera.updateProjectionMatrix();
  render();
}

function clearGroup() {
  built.clear();
  if (!scene || !group) return;
  scene.remove(group);
  group.traverse((obj) => {
    const mesh = obj as THREE.Mesh;
    if (mesh.geometry) mesh.geometry.dispose();
    const material = mesh.material as THREE.Material | THREE.Material[] | undefined;
    if (Array.isArray(material)) material.forEach((m) => m.dispose());
    else material?.dispose();
  });
  group = null;
}

function fit(all: PreviewGeometry[]) {
  if (!camera || !controls) return;
  const min = new THREE.Vector3(Infinity, Infinity, Infinity);
  const max = new THREE.Vector3(-Infinity, -Infinity, -Infinity);
  for (const geo of all) {
    min.min(new THREE.Vector3(geo.bounds.min[0], geo.bounds.min[1], geo.bounds.min[2]));
    max.max(new THREE.Vector3(geo.bounds.max[0], geo.bounds.max[1], geo.bounds.max[2]));
  }
  if (!Number.isFinite(min.x)) return;

  const size = max.clone().sub(min);
  const center = min.clone().add(max).multiplyScalar(0.5);
  const maxDim = Math.max(size.x, size.y, size.z, 0.001);
  framing = { center, maxDim };

  if (grid) {
    scene?.remove(grid);
    grid.geometry.dispose();
    (grid.material as THREE.Material).dispose();
  }
  grid = new THREE.GridHelper(Math.max(maxDim * 4, 4), 20, 0x2c3a4a, 0x1b242e);
  grid.position.set(center.x, min.y, center.z);
  scene?.add(grid);

  resetView();
}

/** 把相机放回初始取景（双击 / 重置按钮）。 */
function resetView() {
  if (!camera || !controls || !framing) return;
  const { center, maxDim } = framing;
  const fov = (camera.fov * Math.PI) / 180;
  const distance = (maxDim / 2 / Math.tan(fov / 2)) * 1.35;
  const direction = new THREE.Vector3(0.45, 0.28, 1).normalize();

  controls.target.copy(center);
  camera.position.copy(center).addScaledVector(direction, distance);
  camera.near = Math.max(distance / 200, 0.001);
  camera.far = distance * 6 + maxDim * 4;
  camera.updateProjectionMatrix();
  controls.minDistance = maxDim * 0.25;
  controls.maxDistance = maxDim * 10;
  controls.update();
  updateZoom();
  render();
}

/** 当前缩放百分比（100 = 刚导入时的取景），只用于界面提示。 */
function updateZoom() {
  if (!camera || !controls || !framing) return;
  const fov = (camera.fov * Math.PI) / 180;
  const base = (framing.maxDim / 2 / Math.tan(fov / 2)) * 1.35;
  const distance = Math.max(camera.position.distanceTo(controls.target), 1e-6);
  const pct = Math.round((base / distance) * 100);
  if (pct !== zoomPct.value) zoomPct.value = pct;
}

/** 给没有滚轮的场景准备的放大/缩小。 */
function zoomBy(factor: number) {
  if (!controls) return;
  if (factor > 1) controls.dollyIn(1 / factor);
  else controls.dollyOut(factor);
  updateZoom();
  render();
}

/**
 * 自己处理滚轮：WebView2 下 deltaY 的单位/步长不统一（有时是小整数“行”），
 * 交给 OrbitControls 会出现“滚了没反应”。这里换算成指数步进。
 */
function onWheel(event: WheelEvent) {
  if (!controls) return;
  event.preventDefault();
  const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 100 : 1;
  const delta = event.deltaY * unit;
  if (!delta) return;
  const factor = Math.exp(-delta / 400); // 滚轮向上(负) = 拉近
  if (factor < 1) controls.dollyOut(factor);
  else controls.dollyIn(1 / factor);
  updateZoom();
  render();
}

function build(all: PreviewGeometry[]) {
  if (!scene) return;
  clearGroup();
  if (!all.length) {
    render();
    return;
  }

  group = new THREE.Group();
  for (const geo of all) {
    if (!geo.positions.length || !geo.indices.length) continue;
    const buffer = new THREE.BufferGeometry();
    buffer.setAttribute("position", new THREE.BufferAttribute(geo.positions, 3));
    if (geo.normals) buffer.setAttribute("normal", new THREE.BufferAttribute(geo.normals, 3));
    if (geo.uvs) buffer.setAttribute("uv", new THREE.BufferAttribute(geo.uvs, 2));
    buffer.setIndex(new THREE.BufferAttribute(geo.indices, 1));
    if (!geo.normals) buffer.computeVertexNormals();
    buffer.computeBoundingSphere();

    const material = new THREE.MeshStandardMaterial({
      color: props.selected.includes(geo.name) ? COLOR_PICKED : COLOR_IDLE,
      metalness: 0.12,
      roughness: 0.7,
      side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(buffer, material);
    mesh.name = geo.name;
    mesh.visible = !props.hidden.includes(geo.name);
    built.set(geo.name, mesh);
    group.add(mesh);
  }
  scene.add(group);
  fit(all);
  render();
}

function syncVisibility() {
  for (const [name, mesh] of built) mesh.visible = !props.hidden.includes(name);
  render();
}

/** 勾选的网格亮蓝，没勾的压成灰色——和列表里的勾选状态一致。 */
function syncColors() {
  for (const [name, mesh] of built) {
    const material = mesh.material as THREE.MeshStandardMaterial;
    const picked = props.selected.includes(name);
    material.color.setHex(picked ? COLOR_PICKED : COLOR_IDLE);
    material.emissive.setHex(picked ? 0x0d2436 : 0x000000);
  }
  render();
}

onMounted(() => {
  const el = host.value;
  if (!el) return;

  renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.domElement.className = "meshviewer__canvas";
  el.appendChild(renderer.domElement);

  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(40, 1, 0.01, 1000);
  camera.position.set(0, 1, 3);

  scene.add(new THREE.HemisphereLight(0xdfe9ff, 0x1a2230, 1.35));
  const key = new THREE.DirectionalLight(0xffffff, 2.1);
  key.position.set(3, 6, 4);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0x7fb8ff, 1.1);
  rim.position.set(-4, 2, -5);
  scene.add(rim);

  controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  // 滚轮由 onWheel 统一处理，避免同一个事件被缩放两次
  controls.enableZoom = false;
  controls.screenSpacePanning = true;

  // 阻尼是在 update() 里结算的：没有逐帧 update，拖拽松手后不会收敛
  const tick = () => {
    raf = requestAnimationFrame(tick);
    if (!controls || !camera) return;
    if (controls.update()) {
      updateZoom();
      render();
    }
  };
  tick();

  renderer.domElement.addEventListener("wheel", onWheel, { passive: false });
  renderer.domElement.addEventListener("dblclick", resetView);

  observer = new ResizeObserver(resize);
  observer.observe(el);
  resize();

  if (props.meshes.length) build(props.meshes);
});

watch(
  () => props.meshes,
  (all) => build(all),
);
watch(() => props.hidden, syncVisibility, { deep: true });
watch(() => props.selected, syncColors, { deep: true });

onBeforeUnmount(() => {
  cancelAnimationFrame(raf);
  renderer?.domElement.removeEventListener("wheel", onWheel);
  renderer?.domElement.removeEventListener("dblclick", resetView);
  observer?.disconnect();
  controls?.dispose();
  clearGroup();
  renderer?.dispose();
  renderer?.domElement.remove();
  renderer = null;
  scene = null;
  camera = null;
  controls = null;
});
</script>

<template>
  <div ref="host" class="meshviewer">
    <div v-if="!meshes.length && !loading" class="meshviewer__empty">3D</div>
    <div v-if="loading" class="meshviewer__loading"><span class="spinner" /></div>
    <div v-if="meshes.length" class="meshviewer__zoom">
      <button
        type="button"
        :title="t('viewer.zoomIn')"
        :aria-label="t('viewer.zoomIn')"
        @click="zoomBy(1.2)"
      >
        &#xFF0B;
      </button>
      <button
        type="button"
        :title="t('viewer.zoomOut')"
        :aria-label="t('viewer.zoomOut')"
        @click="zoomBy(1 / 1.2)"
      >
        &#xFF0D;
      </button>
    </div>
    <button
      v-if="meshes.length"
      class="meshviewer__reset"
      type="button"
      :title="t('viewer.reset')"
      :aria-label="t('viewer.reset')"
      @click="resetView"
    >
      <span class="glyph">&#xE72C;</span>
    </button>
    <p v-if="meshes.length" class="meshviewer__hint">
      {{ t("viewer.hint") }} · {{ zoomPct }}%
    </p>
  </div>
</template>

<style scoped>
.meshviewer {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 220px;
  border-radius: 10px;
  overflow: hidden;
  background:
    radial-gradient(120% 90% at 50% 0%, rgba(76, 194, 255, 0.08), transparent 60%),
    var(--card-bg);
  border: 1px solid var(--stroke);
}

.meshviewer :deep(canvas) {
  display: block;
  width: 100% !important;
  height: 100% !important;
}

.meshviewer__empty {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  font-size: 12px;
  letter-spacing: 0.2em;
  color: var(--text-3);
  opacity: 0.6;
}

.meshviewer__loading {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  background: rgba(8, 12, 18, 0.35);
}

.meshviewer__reset {
  position: absolute;
  right: 8px;
  top: 8px;
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  padding: 0;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 7px;
  background: rgba(12, 18, 26, 0.72);
  color: var(--text-2, #9fb0c7);
  cursor: pointer;
}

.meshviewer__reset:hover {
  color: var(--text-1);
  border-color: var(--accent, #4cc2ff);
}

.meshviewer__reset .glyph {
  font-size: 13px;
}

.meshviewer__zoom {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.meshviewer__zoom button {
  width: 22px;
  height: 22px;
  display: grid;
  place-items: center;
  padding: 0;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 6px;
  background: rgba(12, 18, 26, 0.72);
  color: var(--text-2, #9fb0c7);
  font-size: 12px;
  line-height: 1;
  cursor: pointer;
}

.meshviewer__zoom button:hover {
  color: var(--text-1);
  border-color: var(--accent, #4cc2ff);
}

.meshviewer__hint {
  position: absolute;
  left: 10px;
  bottom: 6px;
  margin: 0;
  font-size: 10px;
  color: var(--text-3);
  opacity: 0.55;
  pointer-events: none;
  user-select: none;
}

.spinner {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.16);
  border-top-color: var(--accent, #4cc2ff);
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
