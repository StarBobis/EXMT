# EXMT Modbox · 角色目录（自动生成）

> 数据来源：`LocalCache/Data/AssetBundles_Windows` 全量深度扫描（9613 个 bundle，8 进程约 5 分钟）。
> 生成脚本：`C:\DevCache\gf2mod\deep_scan2.py` → `analyze_best.py` → `build_catalog.py`。

## 1. 结果

| 指标 | 数量 |
| --- | --- |
| 含 `c_<Name>_<slg|dorm>_<part>_lod0` 网格的角色 | **240** |
| 其中有 body 网格 | 210 |
| 其中有 hair 网格 | 231 |
| 其中有 cloth 网格 | 74 |
| 提取到头像的角色 | **231**（9 个是 Commander/Helena 变体，缓存里没有头像） |
| 说明 | 9 个缺头像的用同角色其它变体的头像替补，仍缺的显示首字 |

输出：`src/data/catalog.json` + `public/portraits/*.png`（128×128）。

## 2. 目录字段

```json
{
  "id": "c_VepleySSR01",
  "name": "VepleySSR01",
  "display": "维普蕾",
  "variant": "slg",
  "bundle": "470d4627…",
  "parts": {"body": "c_VepleySSR01_slg_body_lod0", "cloth": "…", "hair": "…"},
  "part_bundles": {"body": ["470d…", "65c5…", "872f1c4f…"]},
  "portrait": {"bundle": "…", "texture": "Avatar_Head_VepleySSR"}
}
```

**`part_bundles` 很重要**：同一个网格可能同时存在于 2–3 个 bundle 里（本项目 700 多个部位中有 51 个是这种情况）。
只改一个包可能改到"游戏不加载的那一份"，所以安装时要**把每一份都打一遍**。

## 3. 头像从哪来

- 命名规律：`Avatar_Head_<Name>` / `Avatar_Bust_<Name>` / `Avatar_Whole_<Name>` / `Avatar_Gacha_<Name>`，共 1356 张贴图
- 格式：**BC7（format 25）、128×128、单 mip**
- 解码链：`Bundle` 解密 → `texture_info`/`texture_payload` 取 `.resS` 片段 →
  `texture2ddecoder.decode_bc7` → BGRA → **竖直翻转** → PNG
- 竖直翻转是必须的：Unity 纹理按 bottom-up 存储，不翻会得到倒脸
- 匹配失败时按 base name 前缀回退（`BastiDorm` → `Avatar_Head_Basti*`），再不行借用同 base 的其它变体

## 4. 中文名

`src/data/aliases.json`：codename base → 中文（约 70 个常用角色）。
生成时 `display = 中文 + 「· 宿舍」/「· 灵知」/「·2」` 等后缀；没有别名的直接显示 codename。

## 5. 已知限制

1. 目录是**本机缓存快照**：游戏没下载过的角色不会出现，需要重新扫描（`deep_scan2.py`）
2. 只识别 `*_lod0` 基础网格；`_lod1`、武器（`cw_*`）、怪物（`Monster_*`）未纳入
3. 头像没有中文名来源（游戏本地化表未解析），中文名来自手工别名表
4. 240 个角色里 74 个有独立 cloth；其余角色的衣服被烘焙进 body 网格，无法单独替换
