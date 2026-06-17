import buttonSound from '../../sounds/freesound_gamestudio-button-394464.mp3';
import clearComboSound from '../../sounds/freesound_gamestudio-clear-combo-4-394493.mp3';
import itemPickUpSound from '../../sounds/freesound_community-item-pick-up-38258.mp3';

export function playButtonSound(): Promise<void> {
  return playSound(buttonSound);
}

export function playClearComboSound(): Promise<void> {
  return playSound(clearComboSound);
}

export function playItemPickUpSound(): Promise<void> {
  return playSound(itemPickUpSound);
}

function playSound(url: string): Promise<void> {
  if (typeof Audio !== 'function') {
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    let audio: HTMLAudioElement;

    try {
      audio = new Audio(url);
    } catch {
      resolve();
      return;
    }

    const finish = () => resolve();

    const addEventListener =
      typeof audio.addEventListener === 'function' ? audio.addEventListener.bind(audio) : null;

    if (addEventListener) {
      addEventListener('ended', finish, { once: true });
      addEventListener('error', finish, { once: true });
    }

    try {
      const playResult = audio.play();
      if (playResult && typeof playResult.catch === 'function') {
        if (addEventListener) {
          playResult.catch(finish);
        } else {
          playResult.then(finish, finish);
        }
      } else if (!addEventListener) {
        finish();
      }
    } catch {
      finish();
    }
  });
}
