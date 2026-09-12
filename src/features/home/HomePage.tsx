import { Button } from "../../components/Button";
import { HomeInstances, type HomeInstancesProps } from "./HomeInstances";
import { NewsContent } from "../news/NewsContent";
import type { NewsController } from "../news/useNews";
export function HomePage(
  props: HomeInstancesProps & { onSignIn: () => void; news: NewsController },
) {
  const { launcher, navigate, onSignIn } = props;
  return (
    <>
      <HomeInstances {...props} />
      {!launcher.authStatus.authorized && (
        <section className="mb-8 flex flex-wrap items-center justify-between gap-4 border-b border-border-subtle py-4">
          <div>
            <h2 className="text-navigation">Microsoftアカウント</h2>
            <p className="mt-1 text-small text-text-secondary">
              MinecraftのアカウントをSettingsから管理できます。
            </p>
          </div>
          <Button onClick={onSignIn}>Sign in</Button>
        </section>
      )}
      <section className="mb-8">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-section-title">Recent Screenshots</h2>
          <Button tone="ghost" onClick={() => navigate("Gallery")}>
            View all
          </Button>
        </div>
        <p className="text-text-secondary">
          スクリーンショットは未取得です。現在のアプリは画像の読み込みに対応していません。
        </p>
      </section>
      <section>
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-section-title">News</h2>
          <Button tone="ghost" onClick={() => navigate("News")}>
            View all
          </Button>
        </div>
        <NewsContent news={props.news} limit={3} />
      </section>
    </>
  );
}
