## 18. 在 SerializedFile 里改 Mesh（Unity 2019.4 实测）

**适用**：想离线替换角色网格（换衣服/换配件），且**不改骨架**。

### 18.1 先确认类型树是否内嵌

```python
env = UnityPy.load(io.BytesIO(sf_bytes))
f = list(env.files.values())[0]
print(len(f.types))       # >0 = 内嵌类型树 → 可按类型树读写
```

内嵌类型树存在时，`o.read_typetree()` / `o.save_typetree(tree)` 是**逐字节可逆**的
（本项目 1.8 MB 的 Mesh 往返完全一致）。类型树缺失时才需要自己写序列化器。

### 18.2 写回顺序

```
o.save_typetree(tree)      # 只改对象负载
f.save()                   # 重建 SF：对象表/偏移/文件长度自动重算
→ 把新 SF 塞回 bundle node[0]，重压块、重写 blocksInfo、只加密前 32KB
```

`f.save()` 后 metadata 可能比原来多几字节（对象表按 8 字节对齐），`data_offset` 仍是 16 字节对齐，
**不影响加载**。

### 18.3 Mesh 的两个反直觉点

| 点 | 事实 |
| --- | --- |
| `m_IndexBuffer` | 是 **TypelessData（字节数组）**，类型树读出来是 `list[int]` 的字节；必须按 `m_IndexFormat`（0=u16）重新解释，否则索引数会翻倍 |
| `SubMesh.firstByte` | 是**字节**偏移，不是索引序号：`firstByte = 首索引 * (2 或 4)` |
| 顶点流对齐 | **每条流的起始按 16 字节对齐**；stride 本身不补齐。照"stride 也对齐"算会导致 buffer 越界 |

顶点布局由 `m_VertexData.m_Channels`（14 个槽位：0=Position,1=Normal,2=Tangent,3=Color,
4..11=TexCoord0..7,12=BlendWeight,13=BlendIndices）描述，
每个 channel 是 `(stream, offset, format, dimension)`；`format` 0=Float32、1=Float16、10=UInt32。

### 18.4 换衣服 = 只换权重索引，不动绑定

两套骨架通常**不是同一份**（本项目：SSR 服 126 骨、宿舍服 43 骨）。做法：

1. `m_BindPose` 求逆 → 每根骨的世界静止位置
2. **先按 `m_BoneNameHashes` 精确配对**（Unity 的骨骼名哈希 = CRC32，跨包稳定）
3. 剩下的按最近位置配对
4. 权重按映射重写骨骼索引 → 合并重复 → 取前 4 → 归一化

**只按位置配对会翻车**：袖子/手指会绑到邻近的错误骨骼，表现为拉长的飘带。
本项目加哈希后 24/43 精确命中，碎片明显减少。

### 18.5 贴图：同格式同尺寸可直接搬字节

若源/目标贴图格式（如 DXT1=10）与尺寸、mip 数一致，`m_CompleteImageSize` 就相等，
可以整段替换 `.resS` 里的 payload，**完全不需要 DXT/BC 编码器**。
判据：`src.size == dst.size`；换格式或换尺寸时才需要编码器。

### 18.6 验证清单

- [ ] `read_typetree → save_typetree` 对未修改对象逐字节一致
- [ ] 重打包后重新解析：顶点数/索引数/子网格/骨骼数符合预期
- [ ] 只改**顶点坐标**（如放大 1.25 倍）先做一次"可见但低风险"的验证，确认游戏能加载
- [ ] 再上"换整块网格 + 换权重"
- [ ] 游戏日志里 `Failed to load` / `not built with the right version` 为 0

## 19. 把外部网格绑到游戏骨架（Blender 侧）

换"网上下的衣服"时通常拿到的是**未绑定**的 glb/fbx/obj，需要两步：

### 19.1 传权重：DataTransfer 只写"已存在"的顶点组

```python
for vg in src.vertex_groups:                    # 先按名字建组，否则权重写不进去
    if vg.name not in dst.vertex_groups:
        dst.vertex_groups.new(name=vg.name)
mod = dst.modifiers.new("wt", "DATA_TRANSFER")
mod.object = src
mod.use_vert_data = True
mod.data_types_verts = {"VGROUP_WEIGHTS"}
mod.vert_mapping = "POLYINTERP_NEAREST"
mod.layers_vgroup_select_src = "ALL"
bpy.ops.object.modifier_apply(modifier=mod.name)
```

**判据**：`len(dst.vertex_groups)` 应等于来源网格的组数，且 90% 以上顶点 `len(v.groups) > 0`。
本项目实测：不预建组 → 0 个组、0 个有权重的顶点；预建后 126 组、96/96。

### 19.2 绑骨架

把目标网格 `parent` 到来源网格的 Armature，并加一条 `ARMATURE` 修改器（不要用自动权重，
自动权重会重新算一套和游戏骨骼顺序不一致的权重）。

### 19.3 坐标系与缩放

外部模型常见三种坑：**Y-up 没转**（高度落在 Y 轴）、**单位是 1/100**（尺寸 0.01 级）、
**原点不在脚下**。先用 bbox 打印 `size/center` 判断，再旋转 + 等比缩放 + 平移对齐，
最后用一次小分辨率渲染（`BLENDER_EEVEE`，Blender 5.x 不再叫 `BLENDER_EEVEE_NEXT`）确认贴合。

### 19.4 UV 原点：glTF 左上、Unity 左下

从 glb/gltf 拿来的网格，UV 是 **top-left 原点**；Unity 贴图是 **bottom-left**。
不翻转时轮廓完全正确、但颜色错位，典型症状是"身上出现大片皮肤色/纯色碎片"（采样到了图集别的区域）。

```python
uv[:, 1] = 1.0 - uv[:, 1]     # 导入外部 glTF 网格后必做
```

判据：替换后颜色对不上但形状对得上 → 先怀疑 V 翻转，不要先怀疑权重。

### 19.5 贴图槽是定长的：BC1 + 完整 mip 链要精确相等

游戏贴图槽（如 DXT1 2048² + 12 级 mip = **2,796,216** 字节）是固定的，
外部贴图必须编码成**完全相同的字节数**才能整段替换。做法：

1. 目标尺寸取 `m_Width`，mip 级数 = `log2(width)+1`
2. 每级做 2×2 盒式降采样；尺寸 <4 的级补齐成 4×4 块（仍是 8 字节）
3. BC1 编码：块内 min/max 端点 → RGB565，`c0 > c1` 走 4 色模式，逐像素取最近色
4. 写完断言 `len(payload) == m_CompleteImageSize`

min/max 端点编码质量一般但足够；不需要外部编码器依赖。

