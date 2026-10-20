<script setup lang="ts">
/**
 * three.js preview of a whole skin.
 *
 * Every mesh of the selected skin is rendered together in one scene (bind pose,
 * same coordinate space), so the character is complete instead of one part at a
 * time. Geometry arrives as typed arrays from the Python core.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { t } from "../lib/i18n";
import type { PreviewGeometry } from "../lib/preview";

const props = defineProps<{
  meshes: PreviewGeometry[];
  hidden: string[];
  selected: string[];
  /** 当前正在查看贴图的子网格（只加一层淡蓝，不改变导出选中态） */
  active?: string | null;
  /** mesh name → `data:` URL of its diffuse texture */
  maps: Record<string, string>;
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
let shadow: THREE.Mesh | null = null;
let raf = 0;
const built = new Map<string, THREE.Mesh>();
/** what the scene actually holds — one-line footer, so "missing part" is provable */
const stats = ref({ built: 0, tris: 0, box: "-" });
const triCount = computed(() => stats.value.tris);
const boxText = computed(() => stats.value.box);
/** 相机取景参数，供「重置视角」和缩放百分比复用。 */
let framing: { center: THREE.Vector3; maxDim: number } | null = null;

const COLOR_PICKED = 0xffffff;
const COLOR_ACTIVE = 0xc4d5e4;
const COLOR_IDLE = 0x6c737c;
const textureLoader = new THREE.TextureLoader();

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

  // 柔和接触阴影，让角色不是“浮”在网格上
  if (shadow) {
    scene?.remove(shadow);
    shadow.geometry.dispose();
    (shadow.material as THREE.Material).dispose();
  }
  const shadowSize = Math.max(maxDim * 1.6, 1);
  shadow = new THREE.Mesh(
    new THREE.PlaneGeometry(shadowSize, shadowSize),
    new THREE.MeshBasicMaterial({
      map: shadowTexture(),
      transparent: true,
      depthWrite: false,
      opacity: 0.85,
    }),
  );
  shadow.rotation.x = -Math.PI / 2;
  shadow.position.set(center.x, min.y + 0.002, center.z);
  scene?.add(shadow);

  resetView();
}

/** 径向渐变画出来的软阴影贴图（不需要阴影贴图开销）。 */
let shadowMap: THREE.Texture | null = null;
function shadowTexture(): THREE.Texture {
  if (shadowMap) return shadowMap;
  const size = 256;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const ctx = canvas.getContext("2d");
  if (ctx) {
    const gradient = ctx.createRadialGradient(size / 2, size / 2, 0, size / 2, size / 2, size / 2);
    gradient.addColorStop(0, "rgba(0,0,0,0.55)");
    gradient.addColorStop(0.55, "rgba(0,0,0,0.22)");
    gradient.addColorStop(1, "rgba(0,0,0,0)");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, size, size);
  }
  shadowMap = new THREE.CanvasTexture(canvas);
  shadowMap.colorSpace = THREE.SRGBColorSpace;
  return shadowMap;
}

/** 把相机放回初始取景（双击 / 重置按钮）。 */
function resetView() {
  if (!camera || !controls || !framing) return;
  const { center, maxDim } = framing;
  const fov = (camera.fov * Math.PI) / 180;
  const distance = (maxDim / 2 / Math.tan(fov / 2)) * 1.35;
  // 从角色正前方略微偏右上方看（Unity 模型朝 -Z）
  const direction = new THREE.Vector3(0.32, 0.2, -1).normalize();

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
  stats.value = { built: 0, tris: 0, box: "-" };
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
    // Pick the smallest index type that fits. A Uint32Array index buffer is
    // unsupported on WebGL1 without OES_element_index_uint, and then the mesh
    // draws as shredded triangles — which reads as "the mesh is broken" even
    // though the data is fine.
    let maxIndex = 0;
    for (let i = 0; i < geo.indices.length; i += 1) {
      if (geo.indices[i] > maxIndex) maxIndex = geo.indices[i];
    }
    buffer.setIndex(
      new THREE.BufferAttribute(
        maxIndex > 65535 ? new Uint32Array(geo.indices) : new Uint16Array(geo.indices),
        1,
      ),
    );
    // always rebuild normals: the stored normal stream is right for the game's
    // shader layout but some skinned meshes end up facing away from every light
    // in this preview and render black (looks like the part is missing)
    buffer.computeVertexNormals();
    buffer.computeBoundingSphere();

    const url = props.maps[geo.name];
    const map = url ? textureLoader.load(url, () => render()) : null;
    if (map) {
      map.colorSpace = THREE.SRGBColorSpace;
      map.anisotropy = renderer?.capabilities.getMaxAnisotropy() ?? 4;
    }

    const picked = props.selected.includes(geo.name);
    const isActive = props.active === geo.name;
    // a submesh with a diffuse map shows the real base colour even when it is not
    // picked; the grey tint is only for parts that have no texture at all
    const baseColor = map ? 0xffffff : picked ? COLOR_PICKED : isActive ? COLOR_ACTIVE : COLOR_IDLE;
    const material = new THREE.MeshStandardMaterial({
      color: baseColor,
      emissive: picked ? 0x0a1a26 : isActive ? 0x0a2436 : 0x000000,
      map,
      metalness: 0.05,
      roughness: 0.72,
      side: THREE.DoubleSide,
      // no alphaTest: the diffuse textures are stored as BC3/BC7 where the alpha
      // channel is often all-zero (unused), and testing it discarded most of the
      // surface — which looked exactly like "half the mesh is missing"
      alphaTest: 0,
    });
    const mesh = new THREE.Mesh(buffer, material);
    mesh.name = geo.name;
    mesh.visible = !props.hidden.includes(geo.name);
    built.set(geo.name, mesh);
    group.add(mesh);
  }
  scene.add(group);
  let tris = 0;
  let box = new THREE.Box3();
  let first = true;
  for (const [name, mesh] of built) {
    const geometry = (mesh as THREE.Mesh).geometry as THREE.BufferGeometry;
    const index = geometry.getIndex();
    tris += index ? index.count / 3 : (geometry.getAttribute("position")?.count ?? 0) / 3;
    (mesh as THREE.Mesh).updateMatrixWorld(true);
    const bb = new THREE.Box3().setFromObject(mesh as THREE.Mesh);
    if (!bb.isEmpty()) {
      box = first ? bb.clone() : box.union(bb);
      first = false;
    }
    void name;
  }
  stats.value = {
    built: built.size,
    tris: Math.round(tris),
    box: first
      ? "-"
      : `${(box.max.x - box.min.x).toFixed(2)}×${(box.max.y - box.min.y).toFixed(2)}×${(box.max.z - box.min.z).toFixed(2)} m`,
  };
  fit(all);
  render();
}

function syncVisibility() {
  for (const [name, mesh] of built) mesh.visible = !props.hidden.includes(name);
  render();
}

/** 勾选的子网格用原色贴图，没勾的压暗，列表和 3D 的选中状态一致。 */
function syncColors() {
  for (const [name, mesh] of built) {
    const material = mesh.material as THREE.MeshStandardMaterial;
    const picked = props.selected.includes(name);
    const isActive = props.active === name;
    const textured = Boolean(material.map);
    material.color.setHex(
      textured ? 0xffffff : picked ? COLOR_PICKED : isActive ? COLOR_ACTIVE : COLOR_IDLE,
    );
    material.emissive.setHex(picked ? 0x0a1a26 : isActive ? 0x0a2436 : 0x000000);
  }
  render();
}

onMounted(() => {
  const el = host.value;
  if (!el) return;

  renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  // 影视调色：ACES 高光滚降，避免纯白过曝
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.2;
  renderer.domElement.className = "meshviewer__canvas";
  el.appendChild(renderer.domElement);

  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(40, 1, 0.01, 1000);
  camera.position.set(0, 1, 3);

  // 环境反射（PBR 需要）：给金属/粗糙面一点棚拍的层次
  const pmrem = new THREE.PMREMGenerator(renderer);
  scene.environment = pmrem.fromScene(new RoomEnvironment(), 0.04).texture;
  scene.environmentIntensity = 0.75;
  pmrem.dispose();

  // 三点布光：主光（右前上，暖）/ 补光（左前，冷、弱）/ 轮廓光（后上，强）
  const key = new THREE.DirectionalLight(0xfff0dc, 4.2);
  key.position.set(3.0, 4.2, -3.0);
  scene.add(key);
  const fill = new THREE.DirectionalLight(0x9fc2ff, 1.8);
  fill.position.set(-3.5, 1.6, -2.5);
  scene.add(fill);
  const rim = new THREE.DirectionalLight(0xdcecff, 4.5);
  rim.position.set(-1.8, 3.2, 4.0);
  scene.add(rim);
  const top = new THREE.DirectionalLight(0xffffff, 1.1);
  top.position.set(0, 6, -0.4);
  scene.add(top);
  scene.add(new THREE.HemisphereLight(0xdfe9ff, 0x121820, 0.7));

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
watch(() => props.maps, () => build(props.meshes), { deep: true });
watch(() => props.hidden, syncVisibility, { deep: true });
watch(() => props.selected, syncColors, { deep: true });
watch(() => props.active, syncColors);

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
    <!-- debug: what the scene actually got, so "missing part" can be told apart
         from "part is in the scene but out of frame / unlit" -->
    <div v-if="meshes.length" class="meshviewer__stats">
      {{ stats.built }}/{{ meshes.length }} · {{ triCount }} 面 · {{ boxText }}
    </div>
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
.meshviewer__stats {
  position: absolute;
  left: 10px;
  top: 8px;
  font-size: 10px;
  color: var(--text-3);
  background: color-mix(in srgb, var(--bg-1) 72%, transparent);
  padding: 2px 6px;
  border-radius: 5px;
  pointer-events: none;
}

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
  left: 8px;
  right: 8px;
  bottom: 6px;
  margin: 0;
  font-size: 10px;
  line-height: 1.4;
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
