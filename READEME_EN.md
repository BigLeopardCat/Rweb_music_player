# Music Player API Documentation

This document describes the HTTP API interface provided by the Music Player, which can be called by external scripts (such as Python) to control the player.

**Base URL**: `http://127.0.0.1:<port>`
*   Default port is `3000`.
*   The port can be modified in the UI (bottom bar) or via the `config.json` file.

---

## 1. Play Music

Control the player to play a specific file or a song at a specific index.
- If `index` is provided, it plays the song at the corresponding index in the specified playlist.
- If `path` is provided, it adds the file to the playlist (**automatically inserted at the top**) and plays it.
- If both are provided, `index` takes precedence.

*   **URL**: `/play`
*   **Method**: `POST`
*   **Content-Type**: `application/json`

### Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `path` | string | No* | Absolute path to the music file. Required if `index` is not provided. |
| `index`| number | No* | Index of the song in the playlist (starts from 0). If provided, `path` is ignored. |
| `playlist` | string | No | Target playlist name. Defaults to the currently selected playlist if omitted. |

### Request Examples

**1. Play a specific file (auto-added to top):**
```json
{
  "path": "D:\\Music\\song.mp3"
}
```

**2. Play the 1st song in the current playlist (index 0):**
```json
{
  "index": 0
}
```

**3. Play the 2nd song in a specific playlist:**
```json
{
  "index": 1,
  "playlist": "My Favorites"
}
```

### Response

*   **Success (200 OK)**:
    ```json
    "Playing in Default Playlist"
    ```
*   **Failure (200 OK)** (File not found or index out of bounds):
    ```json
    "File not found or invalid request"
    ```

### Python Example

```python
# Play a file
client.play_file("D:\\Music\\song.mp3")

# Play the 1st song in current playlist
client.play_index(0)

# Play the 2nd song in "My Favorites"
client.play_index(1, playlist="My Favorites")
```

---

## 2. Get Playlist

Retrieve the content of the current playlist and a list of all available playlists.

*   **URL**: `/playlist`
*   **Method**: `GET`

### Response Example

```json
{
  "current": "Default Playlist",
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
    "Default Playlist",
    "My Favorites"
  ]
}
```

### Python Example

```python
data = client.get_playlist()
print(f"Current Playlist: {data['current']}")
for f in data['files']:
    print(f['name'])
```

---

## 3. Remove from Playlist

Remove a song at a specific index from a specified playlist.

*   **URL**: `/playlist/remove`
*   **Method**: `POST`
*   **Content-Type**: `application/json`

### Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `index`| number | Yes | Index of the song to remove (starts from 0). |
| `playlist` | string | No | Target playlist name. Defaults to the currently selected playlist if omitted. |

### Request Example

```json
{
  "index": 0
}
```

### Response

*   **Success (200 OK)**:
    ```json
    "Removed item 0 from Default Playlist"
    ```
*   **Failure (200 OK)** (Index out of bounds):
    ```json
    "Index out of bounds"
    ```

### Python Example

```python
# Remove the 1st song from current playlist
client.remove_from_playlist(0)

# Remove the 3rd song from "My Favorites"
client.remove_from_playlist(2, playlist="My Favorites")
```

---

## 4. Rename Playlist

Rename an existing playlist.

*   **URL**: `/playlist/rename`
*   **Method**: `POST`
*   **Content-Type**: `application/json`

### Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `old_name` | string | Yes | Original playlist name. |
| `new_name` | string | Yes | New playlist name. |

### Request Example

```json
{
  "old_name": "Default Playlist",
  "new_name": "My Songs"
}
```

### Response

*   **Success**: `"Playlist renamed"`
*   **Failure**: `"Playlist not found"` or `"New name already exists"`

### Python Example

```python
client.rename_playlist("Default Playlist", "New Name")
```

---

## 5. Delete Playlist

Delete a specified playlist. Note: You cannot delete the last remaining playlist.

*   **URL**: `/playlist/delete`
*   **Method**: `POST`
*   **Content-Type**: `application/json`

### Request Parameters

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | string | Yes | Name of the playlist to delete. |

### Request Example

```json
{
  "name": "My Songs"
}
```

### Response

*   **Success**: `"Playlist deleted"`
*   **Failure**: `"Playlist not found"` or `"Cannot delete the last playlist"`

### Python Example

```python
client.delete_playlist("Unwanted Playlist")
```

---

## Appendix: Python Client Wrapper

For convenience, it is recommended to use the following wrapper class (already included in `test_api.py`):

```python
import requests

class MusicPlayerClient:
    def __init__(self, base_url="http://127.0.0.1:3000"):
        # If you changed the port, pass the new URL here, e.g., "http://127.0.0.1:8080"
        self.base_url = base_url

    def _post(self, url, payload):
        try:
            response = requests.post(url, json=payload)
            return response.json()
        except Exception as e:
            return {"error": str(e)}

    def play_file(self, path, playlist=None):
        """Play a specific file"""
        payload = {"path": path}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/play", payload)

    def play_index(self, index, playlist=None):
        """Play a specific index"""
        payload = {"index": int(index)}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/play", payload)

    def get_playlist(self):
        """Get playlist info"""
        try:
            return requests.get(f"{self.base_url}/playlist").json()
        except Exception as e:
            return {"error": str(e)}

    def remove_from_playlist(self, index, playlist=None):
        """Remove song"""
        payload = {"index": int(index)}
        if playlist: payload["playlist"] = playlist
        return self._post(f"{self.base_url}/playlist/remove", payload)

    def rename_playlist(self, old_name, new_name):
        """Rename playlist"""
        payload = {"old_name": old_name, "new_name": new_name}
        return self._post(f"{self.base_url}/playlist/rename", payload)

    def delete_playlist(self, name):
        """Delete playlist"""
        payload = {"name": name}
        return self._post(f"{self.base_url}/playlist/delete", payload)
```
