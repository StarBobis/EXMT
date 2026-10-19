# 从发货包里取 AnimationClip（通用 Unity，实测 GF2/Unity 2019.4）

> 目标：把游戏里**已构建**的动作导出成「每根骨一根 TRS 曲线」，能直接驱动自己的骨架
> （Blender / 引擎无关）。适用于换模型、看蒙皮、做参考动画。

## 1. 先做清单，别先解码

- 动作和网格/贴图一样**散在多个 bundle**，名字通常带资源前缀（GF2：`c_<皮肤>_slg_<动作>[_N]`），
  所以「这个角色有哪些动作」= 按前缀过滤清单，不需要理解动画格式。
- 全库扫描用**两遍法**：第一遍只读「bundle 头 + SerializedFile 头 + type list」，筛出
  **type list 含 `AnimationClip`（class id 74）** 的包；第二遍只对它们整读 + `obj.peek_name()`。
  实测 66 GB / 9613 个包 → **63 秒**（8 进程）→ 39974 个 clip 名 / 2675 个动画包。
  对同样规模无脑 `UnityPy.load` 是 10 分钟级的浪费。
- **别把截断的字节喂给 UnityPy 做第一遍**（最容易踩的一条）：
  `helpers.ImportHelper.check_file_type` 拒绝「声明 `file_size` 大于实际长度」的流，判成
  `FileType.ResourceFile`；`SerializedFile.__init__` 又**急切解析文件里的 AssetBundle 对象**
  （其数据在元数据之后很远）。于是前缀切片要么被静默判成 0 个对象，要么
  `EOFError: read_length out of bounds`，而 `parse_file` 会把它 `except` 掉。
  正确做法是自己读 type list（下一节），并且：**"只解前缀"的取名字手法只对小于前缀的节点成立**，
  大包里的 GameObject 名字 / Transform 数据都在前缀之外，会得到「这个包没有骨骼/没有名字」的假结论。
- 同一个角色可能有多套动画控制器（GF2：`X`、`X_N`、`X_Gen`），动作会散在多个包里；
  别假设「一个角色一个动画包」。

### 1.1 只读元数据判断「这个包有没有 AnimationClip」

`SerializedFile` 头（v21，全大端）：

```
u32 metadata_size | u32 file_size | u32 version | u32 data_offset | u8 endianness(1=大端)
v21: cstring unity_version | i32 target_platform | u8 enable_type_tree | i32 type_count
每个 type: i32 classID | u8 is_stripped | i16 script_type_index | bytes16 old_type_hash
           [enable_type_tree] u32 node_count, u32 stringbuffer_size,
                             node_count x (h B B I I i i i [Q if v>=19]) + stringbuffer
           [v>=21] i32 dep_count, dep_count x u32
```

只需要解压到 `metadata_size` 就能拿到全部 classID（GF2 实测：60 MB 的包只要 86 KB 元数据）。
type tree 按 `node_count × 24/32 + stringbuffer_size` **跳过**即可，不必解析。
v22+ 把前 16 字节的字段挪过位置，别照抄。

## 2. 构建后的 clip 长什么样

关键字段（Unity 2019+，`m_AnimationType` 已经不序列化，`m_MuscleClip` 对 generic clip 也存在）：

```
m_SampleRate, m_MuscleClip.m_StartTime / m_StopTime / m_LoopTime / m_Events
m_MuscleClip.m_Clip.data = {
    m_StreamedClip { data: uint32[], curveCount },
    m_DenseClip    { m_FrameCount, m_CurveCount, m_SampleRate, m_BeginTime, m_SampleArray: float[] },
    m_ConstantClip { data: float[] } }
m_ClipBindingConstant.genericBindings[] = { path, attribute, typeID, ... }
```

**扁平曲线索引空间**（这是唯一容易搞错的地方）：

1. 按 `genericBindings` 的**顺序**把曲线槽分下去。`typeID == 4`（Transform）时按 `attribute`
   占槽：`1 = localPosition` 占 3、`2 = localRotation`（四元数）占 4、`3 = localScale` 占 3、
   `4 = localEulerAnglesRaw` 占 3；其它 typeID 每个绑定占 1。
   （等价定义见 AssetStudio `AnimationClipBindingConstant.FindBinding` 的 `curves += 3/4/1`。）
2. 取值：`index < streamedCurveCount` → streamed；接着 `denseCurveCount` 条 → dense；
   剩下的按顺序取 `m_ConstantClip.data`。
   - streamed：`data` 是 `uint32[]`，按小端重解释成
     `[time f32][numKeys i32] × numKeys{ [curveIndex i32][a,b,c,d f32] }`，
     求值 `((a·dt + b)·dt + c)·dt + d`，`dt = t - key.time`；键之前/之后取端点值（别外推）。
   - dense：帧优先 `m_SampleArray[frame * m_CurveCount + curve]`，帧号 = `(t - m_BeginTime) * m_SampleRate`，
     帧间线性插值。
   - constant：一个值，索引 = `index - streamedCurveCount - denseCurveCount`。

`path` 是 `crc32(变换路径)`。GF2 网格骨骼表用的**就是同一个哈希**，所以在 Unity 游戏里
「动作 → 骨骼」几乎不需要名字：哈希相等即可驱动。

## 3. 三条自证（几十行就能跑，比肉眼可信）

1. **算术自证**：`Σ(每个绑定占的曲线数) == streamedCurveCount + denseCurveCount + len(constant.data)`。
   不等就是 attribute→槽数 或 streamed/dense 顺序错了。
   （GF2 实测：`2455 == 106 + 623 + 1726`。）
2. **四元数模长**：rotation 槽解出来的每帧 `(x,y,z,w)` 模长必须 ≈1
   （GF2 实测 258 根骨 × 61 帧，最大偏差 3.2e-4）。偏了就说明索引错位一两条曲线。
3. **骨骼覆盖率**：拿同角色的网格骨骼表求交集，应接近 100%
   （GF2 实测 13 个网格全部 100%，51/51、136/136）。低于 80% 说明路径哈希口径不同
   （有的项目是 `crc32(短名)`），这时要退回「按路径名匹配」。

## 4. 坑

- **`path == 0` 不是根骨**：`crc32("") == 0` 的绑定在 GF2 里是 Animator 属性（`typeID == 95`），
  不是 Transform。真正的根位移在 `root/Root_M` 这类骨上；用 clip 自带的
  `m_StartX/m_StopX`（起点/终点根变换）和根骨位移的**方向与量级**对一下即可确认。
- **骨骼表在 `Mesh` 上，不在 `SkinnedMeshRenderer` 上**：找 `m_BoneNameHashes` 要遍历 Mesh
  （SMR 上常常是空的，会误判成"这个包没有骨骼信息"）。
- 走 humanoid（`m_MuscleClip.m_DenseClip` 里是 95 条肌肉 + `m_TOS`）的 clip 需要 Avatar 的
  肌肉→骨骼求解，**先判断是哪一种**：绑定表里有 `typeID == 4` 且 attribute ∈ {1,2,3,4}、
  路径数接近整副骨架，就是 generic；只有几十条路径 + attribute 是小整数序列才考虑 humanoid。
- 导出给 Blender 时**按采样率采样**（30–60 Hz 够用）比搬三次键简单且够准；动作只是用来看蒙皮，
  不需要逐位无损（真要无损就存原始 streamed/dense 数据）。
- 采样频率决定体积：258 根骨 × 10 个 float × 30 Hz ≈ 每帧 30 KB、1 秒 0.5 MB（JSON，5 位小数）。
  60 Hz 直接翻倍；粘性小数的 `repr` 会让 JSON 再涨 3 倍，必须 `round()`。

## 5. 骨骼层级：clip 里没有，同包的 prefab 里有

clip 只存 `crc32(路径)`，**不含父子关系**；没有层级就没法把「每骨局部 TRS」合成世界矩阵，
动作会散架。层级在同一个 bundle 的角色 prefab 里（GF2 实测 634 个 `Transform` + 634 个
`GameObject`）：

- 用 `Transform.m_Father`（PathID）+ `Transform.m_GameObject` + `GameObject.m_Name`
  拼出每个 Transform 的完整路径，`crc32(path)` 就是 clip 里的哈希
  （这条链同时也是**骨骼真名字**的来源，比 Avatar 的 `m_TOS` 覆盖面更大）。
- **路径前缀不一样**：prefab 从模型对象开始（`CF_X_slg_skin/root/Root_M/...`），
  clip 从 Animator 根开始（`root/Root_M/...`）。所以要把**每个后缀**都登记一次
  （`_add_path` 那套），否则一条都对不上（实测：只登记完整路径时覆盖率 0%）。
- **同名后缀会撞**：同一个哈希可能在不同裁剪深度被登记多次，深的那次往往算出「父 = 空」。
  必须定优先级：**有父 > 无父**，同条件下取**裁剪最浅**的一次。否则会出现几个假根，
  它们的子树整体平移（GF2 实测：修之前 5 个假根、200 多根骨错位约 1 m）。
- 校验层级对不对，用**刚体不变量**：父子骨的世界距离在整个动作里必须基本不变
  （GF2 实测 702 对父子，中位 0.0000 m、p90 0.0039 m）。变大了就是父子接错。

## 6. 驱动 DCC 里的骨架（Blender 实测）

- **不要做坐标轴转换**：如果网格和骨架都是按 Unity 坐标（Y 上）导入的，动作也照 Unity 坐标用。
- 姿势 = 逐骨合成世界矩阵：`W(bone) = W(parent) · L(bone)`，`L = T·R·S`；
  clip 没驱动的骨用它的**静止局部** `L_rest = W_rest(parent)⁻¹ · W_rest(bone)`。
- Blender 要的是 **pose basis**：`basis = (W(blender_parent) · rest_parent⁻¹ · bone_rest)⁻¹ · W(bone)`。
  注意括号里要用 **Blender 自己的父子链**（它的父可能是推断出来的、甚至为空），
  而不是 clip 的父子链；两条链不一致时用错会得到 1 m 级的假位移。
- **验收自证（最强的一条）**：把动作放在第 1 帧、再和"完全不挂动作"的网格对比顶点位置，
  应该 **0 个顶点移动**（GF2 实测 max 0.0000 m）。动作首帧通常≈绑定姿势，这一条同时验证了
  rest 矩阵、层级、basis 三个环节；换到中间帧再看顶点是否动、动多少。
- Blender **5.2 没有 `action.fcurves`**（4.4 起是分层动作）。正确写法：
  `act.slots.new(id_type="OBJECT", name=...)` → `act.layers.new(...)` →
  `strips.new(type="KEYFRAME")` → `strip.channelbag(slot, ensure=True).fcurves.new(path, index=...)`；
  赋值要 `obj.animation_data.action = act` **且** `obj.animation_data.action_slot = slot`。
  快速填曲线：`kp.add(n)` + `kp.foreach_set("co", 扁平 [frame, value, ...])` + `curve.update()`。
- **零权重的网格不会跟着动**：脸、武器这类刚性挂件在游戏里是挂在某根骨上的，
  网格权重全 0（GF2 实测 face/weapon 全 0），播动作时它们留在原地 → 看起来"头飞了"。
  这类网格要给一个「挂到哪根骨」的约定，别当成蒙皮网格。
- 动作名别带工作空间时间戳/扩展名：用 clip 名本身；重导入用自定义属性
  （`action["exmt_workspace"] = 绝对路径`）标记归属，比按名字前缀清理可靠。
- 每个动作推一条 **muted NLA 轨**，再把第一个设为当前 action：用户一键切换、立刻能看到动。

## 7. 四元数 or 欧拉：同一根骨只会给一种

Unity 的 clip 对同一个 Transform **要么存四元数、要么存欧拉角**，不会同时给
（GF2 实测 258 个 Transform 里 0 个两者都有：180 个四元数、78 个欧拉）。
只认四元数会让那 78 根骨永久停在 rest 姿势。欧拉是 `localEulerAnglesRaw`，
**单位是度**，顺序是 **Z → X → Y**（`R = Ry · Rx · Rz`）。
自证：把欧拉按这个顺序转成矩阵，和 rest 的局部旋转比，GF2 实测中位 **0.0°**。
