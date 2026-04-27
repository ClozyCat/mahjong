type NavigatorWithUserAgentData = Navigator & {
  userAgentData?: {
    mobile?: boolean;
  };
};

type ElementWithFullscreen = HTMLElement & {
  requestFullscreen?: () => Promise<void> | void;
  webkitRequestFullscreen?: () => Promise<void> | void;
};

type DocumentWithFullscreen = Document & {
  fullscreenElement?: Element | null;
  exitFullscreen?: () => Promise<void> | void;
  webkitFullscreenElement?: Element | null;
  webkitExitFullscreen?: () => Promise<void> | void;
};

const MOBILE_USER_AGENT_PATTERN =
  /Android|iPhone|iPad|iPod|Mobile|BlackBerry|IEMobile|Opera Mini/i;

function swallowAsyncResult(result: Promise<void> | void) {
  if (result && typeof (result as Promise<void>).catch === 'function') {
    void (result as Promise<void>).catch(() => undefined);
  }
}

export function isMobileDevice() {
  if (typeof navigator === 'undefined') {
    return false;
  }

  const navigatorWithUserAgentData = navigator as NavigatorWithUserAgentData;
  if (typeof navigatorWithUserAgentData.userAgentData?.mobile === 'boolean') {
    return navigatorWithUserAgentData.userAgentData.mobile;
  }

  return MOBILE_USER_AGENT_PATTERN.test(navigator.userAgent);
}

export function requestFullscreenMode() {
  if (typeof document === 'undefined') {
    return;
  }

  const fullscreenDocument = document as DocumentWithFullscreen;
  const rootElement = document.documentElement as ElementWithFullscreen;
  if (
    fullscreenDocument.fullscreenElement === document.documentElement ||
    fullscreenDocument.webkitFullscreenElement === document.documentElement
  ) {
    return;
  }

  const requestFullscreen = rootElement.requestFullscreen ?? rootElement.webkitRequestFullscreen;
  if (typeof requestFullscreen !== 'function') {
    return;
  }

  swallowAsyncResult(requestFullscreen.call(rootElement));
}

export function isFullscreenModeActive() {
  if (typeof document === 'undefined') {
    return false;
  }

  const fullscreenDocument = document as DocumentWithFullscreen;
  return (
    fullscreenDocument.fullscreenElement === document.documentElement ||
    fullscreenDocument.webkitFullscreenElement === document.documentElement
  );
}

export function exitFullscreenMode() {
  if (typeof document === 'undefined') {
    return;
  }

  const fullscreenDocument = document as DocumentWithFullscreen;
  if (!fullscreenDocument.fullscreenElement && !fullscreenDocument.webkitFullscreenElement) {
    return;
  }

  const exitFullscreen = fullscreenDocument.exitFullscreen ?? fullscreenDocument.webkitExitFullscreen;
  if (typeof exitFullscreen !== 'function') {
    return;
  }

  swallowAsyncResult(exitFullscreen.call(fullscreenDocument));
}
