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

这种重定向放在 **Blender** 里做（约束 + 姿势烘焙 + 权重传递），Python 只负责解包/打包。
纯 numpy 手搓到"看起来对"的代价很高：kNN / envelope / 拉普拉斯平滑三种都试过，都是扭曲的。

## 22. 批量提取贴图：别对每个 texture 都 read_typetree()

把 240 个角色的立绘/头像导出来时，第一版脚本跑了 **15 分钟**；换成 `peek_name()` 之后 **41 秒**。

原因：`o.read_typetree()` 会把 `image data`（每张贴图的完整字节）一起读进 Python。
一个 40 MB 的 bundle 里有 337 张贴图，逐张解析 = 几十 GB 的临时拷贝。

正确写法（UnityPy）：

```python
for o in bundle.objects():
    if o.type.name != "Texture2D":
        continue
    name = o.peek_name()        # 只读 m_Name 字段，不碰 image data
    if name in wanted:
        hit[name] = o.read_typetree()   # 只对真正要导出的那几张做完整解析
```

配套要点：

- **按 bundle 分组**：一个容器只 open 一次，别在循环里 `Bundle(path)` 重开
- 导出的图统一压成 WebP（q90，保留 alpha）；380×752 的立绘约 60 KB
- 同一张图不要分配给多个条目：先精确匹配，再退化匹配，用一个 `used` 集合去重，
  否则界面上会出现"不同皮肤同一张卡"

### 皮肤立绘的命名规律（GF2）

| 前缀 | 内容 | 典型尺寸 |
| --- | --- | --- |
| `Img_ChrSkinPic_<Skin>` | **皮肤立绘**（透明底整身） | 380×752 |
| `Avatar_Half_<Skin>` | 半身 | ~512 |
| `Avatar_Bust_<Skin>` | 胸像 | 512×512 |
| `Avatar_Head_<Skin>` | 头像（列表图标） | 128×128 |
| `Avatar_Gacha_<Skin>` | 抽卡滚动条（**不是立绘**） | 256×1600 |

`Avatar_Gacha_*` 名字最像立绘，但它是抽卡动画用的长条，别拿它当皮肤图。


这种重定向建议放在 **Blender** 里做（有约束、姿势烘焙、权重传递），
Python 只负责解包/打包。纯 numpy 手搓到"看起来对"的代价很高。




## 23. 骨骼数据的边界：只有哈希，没有名字与层级

**症状**：导出的网格在 Blender 里"有骨架但看着不像骨架"——126 根骨堆在一起，没有父子关系，名字是 `b0…b125`。

**机制**：
- `Mesh` 里骨骼信息只有三样：`m_BindPose`（每骨 4×4 逆绑定矩阵）、`m_BoneNameHashes`（CRC32）、`m_RootBoneNameHash`。
- 顶点侧是 `bones (N,4) u32` + `weights (N,4) f32`，指向哈希数组下标。
- **没有** parent 索引、没有名字字符串。名字在 `SkinnedMeshRenderer.bones → Transform` 那一侧，网格本身拿不到。

**结论（可复用）**：
1. 只要拿到 bind pose + 哈希，骨架就"在"文件里；缺的只是**可读性**。
2. 不要用几何距离重建父子（MST/Prim）：会得出 `髋→大腿→手` 这种链，一旋转整条肢体跟着走，比平铺更糟。宁可不连。
3. 导出时额外写一份明文 `skeleton.json`（每骨 index/hash/rest_position + root hash），比让人去翻 npz 强得多。
4. 想要真名字只能**对撞**：拿标准人形骨名（`Bip001 Pelvis`、`J_Bip_C_Hips`、`Hips`、`mixamorig:Hips`…）算 CRC32，与 `m_BoneNameHashes` 求交。

**Blender 侧落地**（`mesh_io.py`）：
```python
for i, m in enumerate(bind):            # bind = m_BindPose
    eb = arm.edit_bones.new("b%03d" % i)
    world = matrix_from_dict(m).inverted()
    eb.head = world.translation
    eb.tail = eb.head + (world.to_3x3() @ Vector((0, 1, 0))) * 0.04
# 哈希挂到骨架自定义属性（uint32 直接存会 OverflowError，用 JSON 字符串）
arm_obj["exmt_bone_hashes"] = json.dumps([int(h) for h in hashes])
arm.display_type = "OCTAHEDRAL"   # 注意是 armature data，不是 object
arm_obj.show_in_front = True
```

**glTF 为什么不够**：glTF 的 skin 只有 joints + inverseBindMatrices，**丢弃** `m_BoneNameHashes`、`m_BonesAABB`、`m_RootBoneNameHash`、通道布局与碰撞体；
一个"看起来对"的 glTF 回灌游戏必然掉字段。交换格式必须自定义且无损。

---

## 24. 改完的网格回灌游戏：什么会"悄悄变掉"

**目标判据**：把**没改过**的网格原样走一遍"导出 → DCC → 打包回 bundle"，拿出来必须**与游戏里那份逐字节相同**。达不到就说明中间有一环在"重算"而不是"搬运"。实测过的三个坑：

| 坑 | 症状 | 修法 |
| --- | --- | --- |
| 子网格 `localAABB` 用 `pos[min:max+1]` 算 | 未改动的网格里，有几个的 `m_SubMeshes` 与游戏原值不同（盒体明显偏大）。Unity 用它做剔除，盒体不准会误剔除/漏剔除 | 按**被这个子网格索引引用到的顶点**算：`pos[indices[first:first+count]]`。改完 12/12 网格字段零差异 |
| DCC 重算法线与切线 | 未改动的网格法线中位偏 **2.3°**、切线中位偏 **11.4°**（还有切线 w 符号翻转）。法线贴图/光照会变，用户会以为"我什么都没改怎么就变了" | 导入时把游戏法线设成 **custom split normals**（Blender：`mesh.normals_split_custom_set_from_vertices`），导出时读 **per-corner** 法线；切线没有 API 可存，就塞进网格属性（`FLOAT_VECTOR` + 一个 `FLOAT` 存 w），导出优先读回，顶点数变了才回退重算 |
| `foreach_get/foreach_set` 写非连续切片 | 切线读回来**全是 0**（写进的是临时拷贝，原数组没动），而且不报错 | 先用 `np.empty((n,3))` 连续数组 get，再整体赋给 `out[:, :3]` |

**验证方法（重要）**：不要比裸 float 的 `max abs diff`——1e-7 的舍入和 90° 的错向在里面长得一样。要比**角度分位数**：
```python
cos = (a[:, :3] * b[:, :3]).sum(1) / (norm(a) * norm(b))
deg = np.degrees(np.arccos(np.clip(cos, -1, 1)))
print(np.percentile(deg, [50, 90, 99]), (deg > 5).mean())   # 期望 p90 ≤ 0.01°
```
位置差 1e-7（float32 最后一位）可以接受，权重/骨骼索引/UV/索引表必须**逐位相等**。

**多网格一起打包**：同一个 bundle 里通常有整族网格（body/cloth1-4/hair…），按 bundle 分组、**一个包只写一次**（否则后写的会覆盖前面刚 patch 的对象）；`.exmtmesh` 里存了完整 typtree，打包时整棵树还原、只替换几何与索引，`m_BindPose`/`m_BoneNameHashes`/碰撞体原样留着。

**打包源必须是原始副本**：如果从"游戏目录当前文件"打包，第二次生成会把上一次的改动烤进去（下载/替换过的包尤其明显）。原始副本（本项目的 `<cache>/game/bundles`）优先，其次旧备份，最后才是游戏目录。

---

## 25. 多个 Mod 同时生效：补丁 + 整合，而不是"覆盖游戏文件"

**症状**：装第二个 Mod 之后第一个失效（"改了没效果"），而且查不出谁覆盖了谁。

**机制**：一个 bundle 在磁盘上只有一份文件，所以"装 N 个 Mod"本质上是**同一文件的 N 次覆盖**，后装赢。更麻烦的是**一个包里常常住着多个角色的资源**（实测 643 个包里 68 个装了两个以上角色的网格），所以"两个不同角色"也可能撞包。

**做法（可复用）**：

1. **Mod = 补丁集合，不是打好的包**。生成时把每处改动的源数据（本项目是 `.exmtmesh`）留在 Mod 目录里，清单记 `{mesh, file, target bundles}`；打好的 bundle 只是缓存。
2. **运行时状态 = 原始包 + 所有启用 Mod 的补丁**，每次"启用/停用"都从原始副本重算一遍再整体写入（本项目 `applyMods`：把所有启用 Mod 的补丁列表合成一个 spec，交给打包器，一个包只写一次）。记录 `applied.json` 保存"哪些包现在被改过、被谁改的"，这样**没有任何 Mod 再改某个包时能把它还原**。
3. **同一网格的多次修改**：按安装时间排序、后来者覆盖，并在 UI 上显示"哪个包被哪些 Mod 改过"。
4. **同名网格的所有副本一起改**（见 §24 末段）：先建 `网格名 → [所有包]` 索引（`peek_name()` 扫 643 个包约 13.5 s），每个副本打一遍。

**坑**：

- 补丁只存在内存/临时目录里 → 想合并就没有素材，必须落盘（本项目 `GeneratedMod/patches/`）。
- 跨副本不能照抄整棵树：同一资产的两次烘焙可能通道布局都不同（实测 UV0 一个 float16 一个 float32，`m_IsReadable` 也不同）。判据：通道布局 + 可读标志 + 骨骼哈希表全同才用源树，否则以**目标树的底**只写几何，并按哈希重映射骨骼索引。
- 停用必须"重算"，不能"还原备份"：还原会把别的 Mod 在同一包里的改动一起冲掉。

---

## 26. DCC 里的"这一坨网格是谁"：别用自定义属性，用集合 + 蓝图

**背景**：要把用户在 Blender 里改过/替换过的物体映射回"游戏里的第几个 Submesh"，最省事的做法是导入时给物体写个自定义属性（`obj["exmt_source"] = "c_X_slg_cloth_lod0"`）。**这条路会在第一次"换成自己的模型"时崩掉**：用户新建的物体没有这个属性，复制/改名/追加也会失配。

**可复用的做法**：

1. **索引由本体分配**，不在 DCC 里猜：导出工作空间时给每个网格写 `index`（0,1,2…）+ `alias`（短名，如 `cloth1_lod0`）。索引一旦写下就是这个工作空间的身份，重命名物体/换模型都不影响。
2. **一个工作空间 = 一个集合**（Collection）：导入的网格、骨架、辅助对象全放进去，并从别的集合解链。两个工作空间并存时不会混，用户也能整组隐藏/删除。
3. **一个工作空间 = 一个蓝图**（自定义 `NodeTree`）：
   - `物体` 节点（选 Object + 选 Submesh 的 EnumProperty，items 动态取自清单，标签就是 `0. body_lod0`）
   - `生成 Mod` 节点，**每个 Submesh 一个输入槽**（动态 socket，槽名同标签）
   - 打包 = 遍历生成节点上的连线：`槽位索引 → 物体 → 导出 → 打到该网格对应的所有 bundle`；没连的槽位不打包（游戏里保持原样）
   - 导入时把每个网格**预连**到自己槽位，所以"什么都不做直接生成"仍然正确；用户要换模型只改那条连线的物体
4. 身份只存在于"连线"里：同一个物体可以接两个槽位（一个物体顶两个 Submesh），也可以两个节点接同一槽位（后连的赢，Blender 的 socket 只允许一条 link）。

**Blender 5.x 的坑**（4.x 不一样，别照抄老教程）：把自定义 NodeTree 显示到节点编辑器里，`SpaceNodeEditor` 已经没有 `ui_type`，`edit_tree`/`id` 都是只读的；必须

```python
area.type = "NODE_EDITOR"
space = area.spaces.active
space.tree_type = "MyBlueprintTree"     # 可写 ENUM
space.node_tree = tree                  # 5.2 的可写指针（4.x 用 edit_tree）
```

**验证方法**：headless 里构造一个非导入物体（立方体）接到某个槽位，生成后检查该 Submesh 的补丁顶点数变成 8、其余网格逐位不变；再把一个槽位断连，确认 `packed` 少一个且该网格不在产物里。


