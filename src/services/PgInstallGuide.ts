import {
  PG_APT_URL,
  PG_DNF_URL,
  PG_MAC_DOWNLOAD_URL,
  PG_WINDOWS_DOWNLOAD_URL,
  POSTGRES_APP_URL,
  WINDOWS_PLATFORM_PATTERN,
} from '../constants/pgTools'
import { MAC_PLATFORM_PATTERN } from '../constants/shortcuts'
import type { DatabaseApi } from './DatabaseApi'

export interface GuideLink {
  label: string
  url: string
}

/** One way to install. `command` is '' when there is nothing to type. */
export interface GuideStep {
  text: string
  command: string
  links: GuideLink[]
}

/** How to install the PostgreSQL client tools of one version on this platform. The app installs nothing itself. */
export class PgInstallGuide {
  private readonly platform: 'mac' | 'windows' | 'linux'
  private readonly api: Pick<DatabaseApi, 'openLink'>

  /** `platform` is `navigator.platform`. */
  constructor(platform: string, api: Pick<DatabaseApi, 'openLink'>) {
    this.api = api
    if (MAC_PLATFORM_PATTERN.test(platform)) {
      this.platform = 'mac'
    } else if (WINDOWS_PLATFORM_PATTERN.test(platform)) {
      this.platform = 'windows'
    } else {
      this.platform = 'linux'
    }
  }

  steps(version: number): GuideStep[] {
    if (this.platform === 'mac') {
      return [
        {
          text: 'Install it with Homebrew:',
          command: `brew install postgresql@${version}`,
          links: [],
        },
        {
          text: 'If Homebrew no longer has it, use Postgres.app or the EDB installer.',
          command: '',
          links: [
            { label: 'Postgres.app', url: POSTGRES_APP_URL },
            { label: 'EDB installer', url: PG_MAC_DOWNLOAD_URL },
          ],
        },
      ]
    }
    if (this.platform === 'windows') {
      return [
        {
          text: 'Run the EDB installer. Tick only "Command Line Tools".',
          command: '',
          links: [{ label: 'Download', url: PG_WINDOWS_DOWNLOAD_URL }],
        },
      ]
    }
    return [
      {
        text: 'Debian and Ubuntu, from the PostgreSQL apt repository:',
        command: `sudo apt install postgresql-client-${version}`,
        links: [{ label: 'Set up the repository', url: PG_APT_URL }],
      },
      {
        text: 'RHEL and Fedora, from the PGDG repository:',
        command: `sudo dnf install postgresql${version}`,
        links: [{ label: 'Set up the repository', url: PG_DNF_URL }],
      },
    ]
  }

  /** Opens a link of the guide in the browser. */
  open(link: GuideLink): Promise<void> {
    return this.api.openLink(link.url)
  }
}
