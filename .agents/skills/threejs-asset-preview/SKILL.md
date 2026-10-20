---
name: threejs-asset-preview
description: Render game/asset data in a three.js preview and make it show real colours — textures shipped as bare base64, empty-sampler black meshes, 32-bit index buffers, wrongly-lighted skinned normals, and alpha-tested BC textures. Use when a 3D preview looks grey/black despite textures existing, when parts look missing, or when a preview must stay interactive on WebGL1/WebView2.
metadata:
  version: "1"
---

# three.js 资产预览：从"灰模"到真实颜色

目标是"点开资产就能看见它本来的颜色"，不是"像个模型"。下面每条都是**看得到的坏** → 判据：

## 1. 贴图存在但模型黑/灰：先查 sampler，不是查灯光

- **裸 base64 不能当 URL**：后端常把 PNG 序列化成 `iVBORw0...`，直接
  `new TextureLoader().load(b64)` 会被当相对路径，永远不解码。
  判据：`map.image`（或 `texture.source.data`）是否为 `undefined`。
  修法：`/^data:/i.test(s) ? s : \`data:image/png;base64,${s}\``，两种形态都接。
- **失败必须解绑**：`map` 绑在材质上但没解码时，WebGL 采样到不完整纹理 → 乘出**纯黑**，
  看起来像"这块没做"。加载失败的 `onError` 里要 `material.map = null` + 回退灰/白基色，
  否则错误会永久隐藏成"模型是黑的"。
- 颜色被基色乘掉：有贴图时基色必须是白色（`0xffffff`），灰基色会把贴图压暗；
  选中高亮不要用基色表达，用 emissive 或描边。

## 2. 面片像被撕碎 / 半块消失

- 索引宽度按最大索引选 `Uint16Array` / `Uint32Array`；WebGL1 没有
  `OES_element_index_uint` 时 32 位索引画成碎片，看起来像数据坏了。
- **不要给这类贴图开 `alphaTest`**：BC3/BC7 的 alpha 通道常全 0（未使用），
  `alphaTest > 0` 会丢掉大半表面——症状就是"只有半个模型"。

## 3. 法线：存储流 vs 预览光

蒙皮网格的 normal stream 面向游戏的 shader 布局，放进预览的三点布光里可能整体背光发黑。
预览里直接 `geometry.computeVertexNormals()`（一次性成本可忽略），别去修数据。

## 4. 验证方式（必须打在像素上）

- 抓**真实窗口**（WebView2 用 CDP `Page.captureScreenshot`），不要只跑类型检查。
- 量化"有没有颜色"：对预览区算饱和度均值/高饱和像素占比。全灰模型约 <0.05，
  有贴图后通常 >0.2；同一机位前后对比才有意义。
- 反证：把 `data:` 前缀故意去掉再抓一张，饱和度必须掉回灰值——证明这条检查抓得住"贴图没加载"。
