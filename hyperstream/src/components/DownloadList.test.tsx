const scrollToIndexMock = jest.fn();

jest.mock('./DownloadItem', () => ({
  DownloadItem: ({ task, isSpotlighted }: { task: { id: string; filename: string }; isSpotlighted?: boolean }) => (
    <div data-testid={`download-item-${task.id}`} data-spotlighted={isSpotlighted ? 'true' : 'false'}>
      {task.filename}
    </div>
  ),
}));

jest.mock('react-virtuoso', () => {
  const React = require('react');

  const Virtuoso = React.forwardRef(({ data, itemContent }: { data: Array<{ id: string }>; itemContent: (index: number, item: unknown) => ReactNode }, ref: ForwardedRef<{ scrollToIndex: typeof scrollToIndexMock }>) => {
    React.useImperativeHandle(ref, () => ({ scrollToIndex: scrollToIndexMock }));
    return <div>{data.map((item, index) => <div key={item.id}>{itemContent(index, item)}</div>)}</div>;
  });

  return { Virtuoso };
});

import { act, type ForwardedRef, type ReactNode } from 'react';
import { createRoot, Root } from 'react-dom/client';
import type { DownloadTask } from '../types';
import { DownloadList } from './DownloadList';

const reactActEnvironment = globalThis as typeof globalThis & {
  IS_REACT_ACT_ENVIRONMENT?: boolean;
};

describe('DownloadList spotlight navigation', () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeAll(() => {
    reactActEnvironment.IS_REACT_ACT_ENVIRONMENT = true;
  });

  afterAll(() => {
    delete reactActEnvironment.IS_REACT_ACT_ENVIRONMENT;
  });

  const tasks: DownloadTask[] = [
    { id: 'active-1', filename: 'active.bin', url: 'https://example.com/active.bin', progress: 25, downloaded: 25, total: 100, speed: 10, status: 'Downloading' },
    { id: 'done-1', filename: 'other.bin', url: 'https://example.com/other.bin', progress: 100, downloaded: 100, total: 100, speed: 0, status: 'Done' },
  ];

  const renderList = async (spotlightRequest?: { taskId: string; token: number } | null) => {
    await act(async () => {
      root.render(
        <DownloadList
          tasks={tasks}
          onPause={jest.fn()}
          onResume={jest.fn()}
          downloadDir="C:/downloads"
          spotlightRequest={spotlightRequest}
        />,
      );
    });
  };

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    scrollToIndexMock.mockReset();
  });

  afterEach(async () => {
    jest.useRealTimers();
    await act(async () => {
      root.unmount();
    });
    container.remove();
  });

  test('reveals a spotlighted task by clearing hiding filters and scrolling it into view', async () => {
    await renderList();

    const searchInput = container.querySelector('input') as HTMLInputElement;
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    setValue?.call(searchInput, 'other');

    await act(async () => {
      searchInput.dispatchEvent(new Event('input', { bubbles: true }));
    });

    const completeButton = Array.from(container.querySelectorAll('button')).find((button) => button.textContent?.includes('Complete'));
    expect(completeButton).toBeTruthy();

    await act(async () => {
      completeButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });

    expect(container.querySelector('[data-testid="download-item-active-1"]')).toBeNull();

    await renderList({ taskId: 'active-1', token: 1 });

    expect((container.querySelector('input') as HTMLInputElement).value).toBe('');
    expect(container.querySelector('[data-testid="download-item-active-1"]')?.getAttribute('data-spotlighted')).toBe('true');
    expect(scrollToIndexMock).toHaveBeenCalledWith({ index: 0, align: 'center' });
  });

  test('removes the spotlight highlight after a short timeout', async () => {
    jest.useFakeTimers();

    await renderList({ taskId: 'active-1', token: 1 });

    expect(container.querySelector('[data-testid="download-item-active-1"]')?.getAttribute('data-spotlighted')).toBe('true');

    await act(async () => {
      jest.advanceTimersByTime(2500);
    });

    expect(container.querySelector('[data-testid="download-item-active-1"]')?.getAttribute('data-spotlighted')).toBe('false');
  });
});