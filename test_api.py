import requests
import sys

class MusicPlayerClient:
    def __init__(self, base_url="http://127.0.0.1:3000"):
        self.base_url = base_url

    def play_file(self, path, playlist=None):
        """播放指定文件，会自动添加到列表顶部"""
        url = f"{self.base_url}/play"
        payload = {"path": path}
        if playlist:
            payload["playlist"] = playlist
        return self._post(url, payload)

    def play_index(self, index, playlist=None):
        """播放指定序号的歌曲"""
        url = f"{self.base_url}/play"
        payload = {"index": int(index)}
        if playlist:
            payload["playlist"] = playlist
        return self._post(url, payload)

    def get_playlist(self):
        """获取播放列表信息"""
        url = f"{self.base_url}/playlist"
        try:
            response = requests.get(url)
            return response.json()
        except Exception as e:
            return {"error": str(e)}

    def remove_from_playlist(self, index, playlist=None):
        """从歌单中删除指定序号的歌曲"""
        url = f"{self.base_url}/playlist/remove"
        payload = {"index": int(index)}
        if playlist:
            payload["playlist"] = playlist
        return self._post(url, payload)

    def rename_playlist(self, old_name, new_name):
        """重命名歌单"""
        url = f"{self.base_url}/playlist/rename"
        payload = {"old_name": old_name, "new_name": new_name}
        return self._post(url, payload)

    def delete_playlist(self, name):
        """删除歌单"""
        url = f"{self.base_url}/playlist/delete"
        payload = {"name": name}
        return self._post(url, payload)

    def _post(self, url, payload):
        try:
            response = requests.post(url, json=payload)
            return response.json()
        except Exception as e:
            return {"error": str(e)}

# 下面是供终端测试用的代码，您在其他代码中只需要 import MusicPlayerClient 即可
if __name__ == "__main__":
    client = MusicPlayerClient()

    if len(sys.argv) < 2:
        print("Usage:")
        print("  python test_api.py list")
        print("  python test_api.py play <path> [playlist_name]")
        print("  python test_api.py index <index> [playlist_name]")
        print("  python test_api.py remove <index> [playlist_name]")
        print("  python test_api.py rename <old_name> <new_name>")
        print("  python test_api.py delete <playlist_name>")
        sys.exit(1)

    cmd = sys.argv[1]
    
    if cmd == "list":
        data = client.get_playlist()
        if "current" in data:
            print(f"Current Playlist: {data['current']}")
            print("Files:")
            for i, file in enumerate(data['files']):
                exists_mark = "" if file.get('exists', True) else " [MISSING]"
                print(f"  [{i}] {file['name']}{exists_mark}")
            print("\nAll Playlists:", data.get('all_playlists', []))
        else:
            print(data)
            
    elif cmd == "play" and len(sys.argv) > 2:
        path = sys.argv[2]
        playlist = sys.argv[3] if len(sys.argv) > 3 else None
        print(client.play_file(path, playlist))
        
    elif cmd == "index" and len(sys.argv) > 2:
        idx = sys.argv[2]
        playlist = sys.argv[3] if len(sys.argv) > 3 else None
        print(client.play_index(idx, playlist))

    elif cmd == "remove" and len(sys.argv) > 2:
        idx = sys.argv[2]
        playlist = sys.argv[3] if len(sys.argv) > 3 else None
        print(client.remove_from_playlist(idx, playlist))

    elif cmd == "rename" and len(sys.argv) > 3:
        old = sys.argv[2]
        new = sys.argv[3]
        print(client.rename_playlist(old, new))

    elif cmd == "delete" and len(sys.argv) > 2:
        name = sys.argv[2]
        print(client.delete_playlist(name))

    else:
        print("Invalid command")
