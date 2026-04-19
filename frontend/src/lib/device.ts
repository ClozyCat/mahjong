type NavigatorWithUserAgentData = Navigator & {
  userAgentData?: {
    mobile?: boolean;
  };
};

type ScreenOrientationWithLock = {
  lock?: (orientation: 'landscape') => Promise<void>;
};

const MOBILE_USER_AGENT_PATTERN =
  /Android|iPhone|iPad|iPod|Mobile|BlackBerry|IEMobile|Opera Mini/i;

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

export function requestLandscapeOrientation() {
  if (typeof window === 'undefined' || !isMobileDevice()) {
    return;
  }

  const orientation = window.screen.orientation as ScreenOrientationWithLock | undefined;
  if (typeof orientation?.lock !== 'function') {
    return;
  }

  void orientation.lock('landscape').catch(() => undefined);
}
