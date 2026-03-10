jest.mock('@tauri-apps/api/core', () => ({
  invoke: jest.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { api } from './commands';

describe('api.startDownload', () => {
  const invokeMock = invoke as jest.Mock;

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  test('forwards expectedChecksum to the Tauri start_download command', async () => {
    await api.startDownload(
      'download-1',
      'https://example.com/file.bin',
      'C:/Downloads/file.bin',
      false,
      { Authorization: 'Bearer token' },
      'sha256:abc123',
    );

    expect(invokeMock).toHaveBeenCalledWith('start_download', {
      id: 'download-1',
      url: 'https://example.com/file.bin',
      path: 'C:/Downloads/file.bin',
      force: false,
      customHeaders: { Authorization: 'Bearer token' },
      expectedChecksum: 'sha256:abc123',
    });
  });

  test('still calls start_download when expectedChecksum is omitted', async () => {
    await api.startDownload(
      'download-2',
      'https://example.com/other.bin',
      'C:/Downloads/other.bin',
    );

    expect(invokeMock).toHaveBeenCalledWith('start_download', {
      id: 'download-2',
      url: 'https://example.com/other.bin',
      path: 'C:/Downloads/other.bin',
      force: undefined,
      customHeaders: undefined,
      expectedChecksum: undefined,
    });
  });
});

