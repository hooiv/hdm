import { buildAuthHeaders, formatSpeed } from '../popup';

describe('popup utilities', () => {
    test('formatSpeed returns human readable string', () => {
        expect(formatSpeed(0)).toBe('');
        expect(formatSpeed(500)).toBe('500.0 B/s');
        expect(formatSpeed(2048)).toBe('2.0 KB/s');
        expect(formatSpeed(5 * 1024 * 1024)).toBe('5.0 MB/s');
    });

    test('buildAuthHeaders includes token and JSON content type only when requested', () => {
        expect(buildAuthHeaders('', false)).toEqual({});
        expect(buildAuthHeaders('secret-token', false)).toEqual({
            'X-HyperStream-Token': 'secret-token'
        });
        expect(buildAuthHeaders('secret-token', true)).toEqual({
            'Content-Type': 'application/json',
            'X-HyperStream-Token': 'secret-token'
        });
    });
});