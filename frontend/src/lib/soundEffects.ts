import clearComboSound from '../../sounds/freesound_gamestudio-clear-combo-4-394493.mp3';
import itemPickUpSound from '../../sounds/freesound_community-item-pick-up-38258.mp3';
import huSound from '../../sounds/puyopuyomegafan1234-winner-game-sound-404167.mp3';
import readyHandSound from '../../sounds/universfield-game-bonus-02-294436.mp3';

export function playClearComboSound(): Promise<void> {
  return playSound(clearComboSound);
}

export function playItemPickUpSound(): Promise<void> {
  return playSound(itemPickUpSound);
}

export function playHuSound(): Promise<void> {
  return playSound(huSound);
}

export function playReadyHandSound(): Promise<void> {
  return playSound(readyHandSound);
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
