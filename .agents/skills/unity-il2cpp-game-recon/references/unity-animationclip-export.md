# 从发货包里取 AnimationClip（通用 Unity，实测 GF2/Unity 2019.4）

> 目标：把游戏里**已构建**的动作导出成「每根骨一根 TRS 曲线」，能直接驱动自己的骨架
> （Blender / 引擎无关）。适用于换模型、看蒙皮、做参考动画。

## 1. 先做清单，别先解码

- 动作和网格/贴图一样**散在多个 bundle**，名字通常带资源前缀（GF2：`c_<皮肤>_slg_<动作>[_N]`），
  所以「这个角色有哪些动作」= 按前缀过滤清单，不需要理解动画格式。
- 全库扫描用**两遍法**（`core/exmt_core/bonenames.py` 的 `_truncated_env` 是现成实现）：
  1. 只读 bundle 头 + 序列化文件元数据前缀（type list 在元数据里），筛出**type list 含
     `AnimationClip`** 的包；这一步不解压正文，几千个包几十秒。
  2. 只对这些包整读，`obj.peek_name()` 收名字。
  全量对 66 GB / 9619 个包做 `UnityPy.load` 是 10 分钟级的浪费。
- 同一个角色可能有多套动画控制器（GF2：`X`、`X_N`、`X_Gen`），动作会散在多个包里；
  别假设「一个角色一个动画包」。

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
- 导出给 Blender 时**按采样率采样**（60 Hz 够用）比搬三次键简单且够准；动作只是用来看蒙皮，
  不需要逐位无损（真要无损就存原始 streamed/dense 数据）。
