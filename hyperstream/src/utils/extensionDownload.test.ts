import { buildExtensionDownloadHeaders } from './extensionDownload';

describe('buildExtensionDownloadHeaders', () => {
  test('filters blank headers and adds pageUrl as Referer when missing', () => {
    expect(buildExtensionDownloadHeaders({ Authorization: 'Bearer test', Empty: '   ' }, 'https://origin.example/page')).toEqual({
      Authorization: 'Bearer test',
      Referer: 'https://origin.example/page',
    });
  });

  test('preserves an existing referer header regardless of casing', () => {
    expect(buildExtensionDownloadHeaders({ referer: 'https://existing.example', Cookie: 'sid=1' }, 'https://fallback.example')).toEqual({
      referer: 'https://existing.example',
      Cookie: 'sid=1',
    });
  });

  test('returns undefined when there is no usable header context', () => {
    expect(buildExtensionDownloadHeaders(null, '   ')).toBeUndefined();
    expect(buildExtensionDownloadHeaders({ Blank: '   ' }, null)).toBeUndefined();
  });
});