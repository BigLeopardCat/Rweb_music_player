# 音乐播放器 API 文档 / Music Player API Documentation

本文档描述了音乐播放器提供的 HTTP API 接口，供外部脚本（如 Python）调用以控制播放器。

This document describes the HTTP API provided by the music player for remote control via external scripts (e.g., Python).

**基础 URL / Base URL**: `http://127.0.0.1:<port>`
*   默认端口为 `3000`。
*   端口可在 UI 界面底部修改，或通过程序自动生成的 `config.json` 文件配置。
*   Default port is `3000`.
*   Port can be changed in the UI or via `config.json`.

---

## 1. 播放音乐 / Play Music

控制播放器播放指定文件或指定序号的音乐。
Control the player to play a specific file or index.

*   **URL**: `/play`
*   **方法 / Method**: `POST`
*   **Content-Type**: `application/json`

### 请求参数 / Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | string | No* | Absolute path of the music file. Required if `index` is not provided. |
| `index`| number | No* | Index of the song in the playlist (0-based). Ignores `path` if provided. |
| `playlist` | string | No | Target playlist name. Defaults to current playlist if omitted. |

### 示例 / Examples

**1. Play file (auto-add to top):**
```json
{
  "path": "D:\\Music\\song.mp3"
}
```

**2. Play index 0 of current playlist:**
```json
{
  "index": 0
}
```

**3. Play index 1 of specific playlist:**
```json
{
  "index": 1,
  "playlist": "My Favorites"
}
```

---

## 2. 获取播放列表 / Get Playlist

获取当前播放列表的内容，以及所有可用歌单的列表。
Retrieve current playlist content and list of all playlists.

*   **URL**: `/playlist`
*   **方法 / Method**: `GET`

### 响应示例 / Response Example

```json
{
  "current": "Default List",
  "files": [
    {
      "path": "D:\\Music\\song1.mp3",
      "name": "song1.mp3",
      "exists": true
    }
  ],
  "all_playlists": [
    "Default List",
    "My Favorites"
  ]
}
```

---

## 3. 从播放列表删除 / Remove from Playlist

从指定歌单中删除指定序号的歌曲。
Remove a song by index from a specific playlist.

*   **URL**: `/playlist/remove`
*   **方法 / Method**: `POST`
*   **Content-Type**: `application/json`

### 请求参数 / Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `index`| number | Yes | Index of the song to remove (0-based). |
| `playlist` | string | No | Target playlist name. Defaults to current playlist. |

---

## 4. 重命名歌单 / Rename Playlist

重命名现有的歌单。
Rename an existing playlist.

*   **URL**: `/playlist/rename`
*   **方法 / Method**: `POST`
*   **Content-Type**: `application/json`

### 请求参数 / Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `old_name` | string | Yes | Original playlist name. |
| `new_name` | string | Yes | New playlist name. |

---

## 5. 删除歌单 / Delete Playlist

删除指定的歌单。
Delete a specific playlist.

*   **URL**: `/playlist/delete`
*   **方法 / Method**: `POST`
*   **Content-Type**: `application/json`

### 请求参数 / Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | Yes | Name of the playlist to delete. |

---

## 6. 切换歌单 / Switch Playlist

切换当前活动的歌单。
Switch the currently active playlist.

*   **URL**: `/playlist/switch`
*   **方法 / Method**: `POST`
*   **Content-Type**: `application/json`

### 请求参数 / Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | Yes | Name of the playlist to switch to. |

---

## 7. Python Client Example

```python
import requests

class MusicPlayerClient:
    def __init__(self, base_url="http://127.0.0.1:3000"):
        self.base_url = base_url

    def _post(self, url, payload):
        try:
            response = requests.post(url, json=payload)
            return response.json()
        except Exception as e:
            return {"error": str(e)}

    def play_file(self, path, playlist=None):
        payload = {"path": path}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/play", payload)

    def play_index(self, index, playlist=None):
        payload = {"index": int(index)}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/play", payload)

    def get_playlist(self):
        try:
            return requests.get(f"{self.base_url}/playlist").json()
        except Exception as e:
            return {"error": str(e)}

    def remove_from_playlist(self, index, playlist=None):
        payload = {"index": int(index)}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/playlist/remove", payload)

    def rename_playlist(self, old_name, new_name):
        payload = {"old_name": old_name, "new_name": new_name}
        return self._post(f"{self.base_url}/playlist/rename", payload)

    def delete_playlist(self, name):
        payload = {"name": name}
        return self._post(f"{self.base_url}/playlist/delete", payload)

    def switch_playlist(self, name):
        payload = {"name": name}
        return self._post(f"{self.base_url}/playlist/switch", payload)
```
