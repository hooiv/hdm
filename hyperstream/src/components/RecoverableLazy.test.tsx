jest.mock('../utils/logger', () => ({
  error: jest.fn(),
}));

import { act } from 'react';
import { createRoot, Root } from 'react-dom/client';
import { RecoverableLazy } from './RecoverableLazy';

describe('RecoverableLazy', () => {
  let container: HTMLDivElement;
  let root: Root;
  let consoleErrorSpy: jest.SpyInstance;

  beforeEach(() => {
    container = document.createElement('div');
    document.body.appendChild(container);
    root = createRoot(container);
    consoleErrorSpy = jest.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(async () => {
    await act(async () => {
      root.unmount();
    });
    consoleErrorSpy.mockRestore();
    container.remove();
  });

  test('retries a failed lazy import and recovers the view', async () => {
    let shouldFail = true;
    const loader = jest.fn(async () => {
      if (shouldFail) {
        throw new Error('chunk download failed');
      }

      return {
        DemoView: ({ label }: { label: string }) => <div>{label}</div>,
      };
    });

    await act(async () => {
      root.render(
        <RecoverableLazy
          loader={loader}
          resolve={(module) => module.DemoView}
          componentProps={{ label: 'Recovered view' }}
          loadingFallback={<div>Loading view...</div>}
          failureTitle="View unavailable"
          failureMessage="Retry to recover the split view."
        />,
      );
    });

    expect(container.textContent).toContain('View unavailable');
    expect(container.textContent).toContain('chunk download failed');
    expect(loader).toHaveBeenCalledTimes(1);

    shouldFail = false;

    await act(async () => {
      container.querySelector('button')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await Promise.resolve();
    });

    expect(loader).toHaveBeenCalledTimes(2);
    expect(container.textContent).toContain('Recovered view');
  });

  test('supports a custom failure renderer for modal-style recovery', async () => {
    let shouldFail = true;
    const loader = jest.fn(async () => {
      if (shouldFail) {
        throw new Error('modal chunk failed');
      }

      return {
        DemoView: ({ label }: { label: string }) => <div>{label}</div>,
      };
    });

    await act(async () => {
      root.render(
        <RecoverableLazy
          loader={loader}
          resolve={(module) => module.DemoView}
          componentProps={{ label: 'Modal recovered' }}
          loadingFallback={<div>Loading modal...</div>}
          failureTitle="Unused default title"
          failureMessage="Unused default message"
          renderFailure={(error, retry) => (
            <div>
              <span>Custom modal recovery</span>
              <span>{error.message}</span>
              <button onClick={retry}>Retry modal</button>
            </div>
          )}
        />,
      );
    });

    expect(container.textContent).toContain('Custom modal recovery');
    expect(container.textContent).toContain('modal chunk failed');
    expect(loader).toHaveBeenCalledTimes(1);

    shouldFail = false;

    await act(async () => {
      container.querySelector('button')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      await Promise.resolve();
    });

    expect(loader).toHaveBeenCalledTimes(2);
    expect(container.textContent).toContain('Modal recovered');
  });
});