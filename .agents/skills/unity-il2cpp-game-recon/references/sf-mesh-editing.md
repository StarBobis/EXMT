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

## 20. 角色全量目录与头像提取

### 20.1 全量扫描的正确姿势

**不要**只 grep 文件头：SerializedFile 的对象名表可能落在 128KB 之外或压缩块里，
本项目用"读前 128KB + 解密前 32KB"只找到 11 个角色，而用 `Bundle(path).objects()` 全解析找到 **240 个**。

```python
b = Bundle(path)                      # 自己解密 + 解块
for o in b.objects():
    if o.type.name == "Mesh": name = o.read_typetree()["m_Name"]
```

9613 个 bundle、8 进程、约 5 分钟（~0.12s/包/进程）。

### 20.2 同一个网格可能存在于多个 bundle

本项目 700 多个部位里 51 个有重复（同一角色同一部位出现在 2–3 个包）。
只改一个包有概率改到"游戏不加载的那一份"，所以**安装时要遍历所有包含该网格的包**。
目录里存 `part_bundles: {part: [hash…]}`。

### 20.3 头像提取（Unity UI 图）

- 命名：`Avatar_Head_<Name>` / `Avatar_Bust_<Name>` / `Avatar_Whole_<Name>`，128×128，**BC7（format 25）**
- 解码：`texture_info` 拿 `.resS` 偏移 → `texture_payload` 切片 →
  `texture2ddecoder.decode_bc7` → BGRA 数组 → **竖直翻转** → PNG
- **Unity 纹理是 bottom-up 存储，必须翻转**，否则头像全是倒的（这个坑在 DXT 贴图上同样存在）

### 20.4 隐藏 / 素体替换

| 需求 | 做法 |
| --- | --- |
| 让某个部位不显示 | 把所有顶点塌缩到 `(0, -10000, 0)`，索引与骨骼不动（比删对象安全） |
| 换素体 | 外部素体按目标 bbox 等比缩放 + 脚落地 + 居中 + **切掉头部三角形** + 最近顶点 kNN 继承权重 |

注意：很多手游角色的 `body` 网格只含**头/手/脚**，躯干在 `cloth` 里；
"隐藏衣服"会露出空身体。素体要贴在 `cloth` 槽（全骨架）而不是 `body` 槽（骨头不全）。

## 21. 外部模型换身体：为什么会炸

### 21.1 失败模式（实测）

| 做法 | 结果 |
| --- | --- |
| 最近顶点 kNN 传权重 | 手臂顶点被绑到躯干/裙骨 → 蒙皮时拉成白色长条、撕裂 |
| 最近骨骼（envelope）传权重 | 不再飞白条，但仍是"面条手臂"：几何在 T-Pose、骨架在 A-Pose，逐骨旋转量不同 |
| kNN + 8 轮拉普拉斯平滑 | 表面连续了，整体仍然扭曲 |

根因不是权重数值，而是**绑定姿势不一致**：
外部网格顶点在它自己的 bind pose 里，游戏写入的是另一套 `m_BindPose`；
蒙皮公式 `P_i · Bind_i · v` 里的 `Bind_i` 一变，同一个顶点就被"预先旋转"到错误位置。

### 21.2 正确公式

```
M  = Σ w_i · (GameBind_i⁻¹ · SourceBind_i)
v' = M · v
```

先把外部网格**逐骨变换进游戏的绑定空间**，再写权重（权重本身可以从最近骨骼取）。

前提是骨骼一一对应。游戏侧只有 `m_BoneNameHashes`（CRC32，不可逆），所以映射只能靠：

1. 外部模型的骨骼名（VRM 的 `J_Bip_L_UpperArm` 之类）→ 人形骨骼语义
2. 用**链结构 + 左右对称性**在游戏骨架上找对应（髋→脊→胸→颈→头、肩→肘→腕、髋→膝→踝）
3. 用两套 bindpose 的静止位置做几何校验

### 21.3 分工建议

这种重定向建议放在 **Blender** 里做（有约束、姿势烘焙、权重传递），
Python 只负责解包/打包。纯 numpy 手搓到"看起来对"的代价很高。



