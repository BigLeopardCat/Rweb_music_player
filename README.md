# 音乐播放器 API 文档

本文档描述了音乐播放器提供的 HTTP API 接口，供外部脚本（如 Python）调用以控制播放器。

**基础 URL**: `http://127.0.0.1:<port>`
*   默认端口为 `3000`。
*   端口可在 UI 界面底部修改，或通过程序自动生成的 `config.json` 文件配置。

---

## 1. 播放音乐

控制播放器播放指定文件或指定序号的音乐。
- 如果提供 `index`，则播放指定歌单中对应序号的音乐。
- 如果提供 `path`，则将文件添加到歌单（**自动置顶**）并播放。
- 如果同时提供，优先使用 `index`。

*   **URL**: `/play`
*   **方法**: `POST`
*   **Content-Type**: `application/json`

### 请求参数

| 字段名 | 类型   | 必填 | 描述                               |
| :----- | :----- | :--- | :--------------------------------- |
| `path` | string | 否*  | 音乐文件的绝对路径。如果未提供 `index`，则此项必填。 |
| `index`| number | 否*  | 歌单中的歌曲序号（从 0 开始）。如果提供了此项，将忽略 `path`。 |
| `playlist` | string | 否 | 目标歌单名称。如果不填，默认为当前选中的歌单。 |

### 请求示例

**1. 播放指定文件（自动添加到列表顶部）：**
```json
{
  "path": "D:\\Music\\song.mp3"
}
```

**2. 播放当前歌单的第 1 首歌（序号 0）：**
```json
{
  "index": 0
}
```

**3. 播放指定歌单的第 2 首歌：**
```json
{
  "index": 1,
  "playlist": "我的最爱"
}
```

### 响应

*   **成功 (200 OK)**:
    ```json
    "Playing in 默认列表"
    ```
*   **失败 (200 OK)** (文件不存在或序号越界):
    ```json
    "File not found or invalid request"
    ```

### Python 调用示例

```python
# 播放文件
client.play_file("D:\\Music\\song.mp3")

# 播放当前歌单第 1 首
client.play_index(0)

# 播放 "我的最爱" 歌单第 2 首
client.play_index(1, playlist="我的最爱")
```

---

## 2. 获取播放列表

获取当前播放列表的内容，以及所有可用歌单的列表。

*   **URL**: `/playlist`
*   **方法**: `GET`

### 响应示例

```json
{
  "current": "默认列表",
  "files": [
    {
      "path": "D:\\Music\\song1.mp3",
      "name": "song1.mp3",
      "exists": true
    },
    {
      "path": "D:\\Music\\missing.mp3",
      "name": "missing.mp3",
      "exists": false
    }
  ],
  "all_playlists": [
    "默认列表",
    "我的最爱"
  ]
}
```

### Python 调用示例

```python
data = client.get_playlist()
print(f"当前歌单: {data['current']}")
for f in data['files']:
    print(f['name'])
```

---

## 3. 从播放列表删除

从指定歌单中删除指定序号的歌曲。

*   **URL**: `/playlist/remove`
*   **方法**: `POST`
*   **Content-Type**: `application/json`

### 请求参数

| 字段名 | 类型   | 必填 | 描述                               |
| :----- | :----- | :--- | :--------------------------------- |
| `index`| number | 是   | 要删除的歌曲序号（从 0 开始）。 |
| `playlist` | string | 否 | 目标歌单名称。如果不填，默认为当前选中的歌单。 |

### 请求示例

```json
{
  "index": 0
}
```

### 响应

*   **成功 (200 OK)**:
    ```json
    "Removed item 0 from 默认列表"
    ```
*   **失败 (200 OK)** (序号越界):
    ```json
    "Index out of bounds"
    ```

### Python 调用示例

```python
# 删除当前歌单第 1 首歌
client.remove_from_playlist(0)

# 删除 "我的最爱" 歌单第 3 首歌
client.remove_from_playlist(2, playlist="我的最爱")
```

---

## 4. 重命名歌单

重命名现有的歌单。

*   **URL**: `/playlist/rename`
*   **方法**: `POST`
*   **Content-Type**: `application/json`

### 请求参数

| 字段名 | 类型   | 必填 | 描述                               |
| :----- | :----- | :--- | :--------------------------------- |
| `old_name` | string | 是 | 原歌单名称。 |
| `new_name` | string | 是 | 新歌单名称。 |

### 请求示例

```json
{
  "old_name": "默认列表",
  "new_name": "我的歌单"
}
```

### 响应

*   **成功**: `"Playlist renamed"`
*   **失败**: `"Playlist not found"` 或 `"New name already exists"`

### Python 调用示例

```python
client.rename_playlist("默认列表", "新歌单名称")
```

---

## 5. 删除歌单

删除指定的歌单。注意：无法删除最后一个剩余的歌单。

*   **URL**: `/playlist/delete`
*   **方法**: `POST`
*   **Content-Type**: `application/json`

### 请求参数

| 字段名 | 类型   | 必填 | 描述                               |
| :----- | :----- | :--- | :--------------------------------- |
| `name` | string | 是 | 要删除的歌单名称。 |

### 请求示例

```json
{
  "name": "我的歌单"
}
```

### 响应

*   **成功**: `"Playlist deleted"`
*   **失败**: `"Playlist not found"` 或 `"Cannot delete the last playlist"`

### Python 调用示例

```python
client.delete_playlist("不需要的歌单")
```

---

## 6. 附录：Python 客户端类封装

为了方便调用，建议使用以下封装类（已包含在 `test_api.py` 中）：

```python
import requests

class MusicPlayerClient:
    def __init__(self, base_url="http://127.0.0.1:3000"):
        # 如果修改了端口，请在初始化时传入新的 URL，例如 "http://127.0.0.1:8080"
        self.base_url = base_url

    def _post(self, url, payload):
        try:
            response = requests.post(url, json=payload)
            return response.json()
        except Exception as e:
            return {"error": str(e)}

    def play_file(self, path, playlist=None):
        """播放指定文件"""
        payload = {"path": path}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/play", payload)

    def play_index(self, index, playlist=None):
        """播放指定序号"""
        payload = {"index": int(index)}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/play", payload)

    def get_playlist(self):
        """获取播放列表"""
        try:
            return requests.get(f"{self.base_url}/playlist").json()
        except Exception as e:
            return {"error": str(e)}

    def remove_from_playlist(self, index, playlist=None):
        """删除歌曲"""
        payload = {"index": int(index)}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/playlist/remove", payload)

    def rename_playlist(self, old_name, new_name):
        """重命名歌单"""
        payload = {"old_name": old_name, "new_name": new_name}
        return self._post(f"{self.base_url}/playlist/rename", payload)

    def delete_playlist(self, name):
        """删除歌单"""
        payload = {"name": name}
        return self._post(f"{self.base_url}/playlist/delete", payload)
```
