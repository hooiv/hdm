import type { DownloadTask } from "../types";
import { findActiveTaskByUrl, isDuplicateDownloadError, normalizeDownloadUrl } from "./downloadDedup";

describe("downloadDedup helpers", () => {
  test("normalizeDownloadUrl strips fragments and default ports", () => {
    expect(normalizeDownloadUrl(" HTTPS://Example.com:443/file.bin?token=1#frag ")).toBe(
      "https://example.com/file.bin?token=1"
    );
  });

  test("isDuplicateDownloadError detects backend duplicate rejections", () => {
    expect(isDuplicateDownloadError("A download for this URL is already active or queued")).toBe(true);
    expect(isDuplicateDownloadError("Failed to connect")).toBe(false);
  });

  test("findActiveTaskByUrl only matches downloading tasks", () => {
    const tasks: DownloadTask[] = [
      {
        id: "done-1",
        filename: "done.bin",
        url: "https://example.com/file.bin",
        progress: 100,
        downloaded: 100,
        total: 100,
        speed: 0,
        status: "Done",
      },
      {
        id: "active-1",
        filename: "active.bin",
        url: "https://example.com:443/file.bin#frag",
        progress: 12,
        downloaded: 12,
        total: 100,
        speed: 10,
        status: "Downloading",
      },
    ];

    expect(findActiveTaskByUrl(tasks, "https://example.com/file.bin")?.id).toBe("active-1");
    expect(findActiveTaskByUrl(tasks, "https://example.com/file.bin", "active-1")).toBeUndefined();
  });
});