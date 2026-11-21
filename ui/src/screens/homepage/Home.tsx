import type { Hello } from "../../api";
import { BrandArt } from "../../components/BrandArt";
import { KitPanel } from "../../components/KitPanel";
import { kitTone } from "../../components/kitTone";
import { Wordmark } from "../../components/Wordmark";
import { hrefOf } from "../../routes";

export interface HomeProps {
  hello: Hello | null;
}

const STEPS = [
  { n: "01", title: "Pick a kit", line: "each kit is a set of tasks" },
  { n: "02", title: "Give it a picture", line: "say or draw what you want" },
  { n: "03", title: "Send it on", line: "check · fix · copy command" },
];

/** Home: the intro, Continue, the kits and their tasks, the pictures. */
export function Home(props: HomeProps) {
  const kits = props.hello?.kits ?? [];
  return (
    <main class="screen home" aria-labelledby="home-title">
      <section class="home__intro">
        <div class="home__lead">
          <div class="home__brand">
            <BrandArt kind="emblem" alt="" />
            <div class="home__words">
              <h1 id="home-title" class="home__title">
                <Wordmark size="home" />
                <span class="visually-hidden"> · Home</span>
              </h1>
              <p class="home__line">Edit pictures and check the result, with models on your Mac or in the cloud.</p>
              <p class="home__promise mono">cost shown before every run · every run recorded · any run copies as a command</p>
            </div>
          </div>
          <ol class="home__steps">
            {STEPS.map((step) => (
              <li key={step.n} class="home__step">
                <span class="home__step-n mono">{step.n}</span>
                <span class="home__step-title">{step.title}</span>
                <span class="home__step-line">{step.line}</span>
              </li>
            ))}
          </ol>
        </div>
        <div class="home__art">
          <BrandArt kind="crew" alt="" />
        </div>
      </section>

      <section class="home__section" aria-labelledby="continue-title">
        <h2 id="continue-title" class="section-title">
          Continue
        </h2>
        <p class="empty">Nothing is running and there are no drafts. A run you start shows here until it ends.</p>
      </section>

      <section class="home__section" aria-labelledby="kits-title">
        <h2 id="kits-title" class="section-title">
          Kits
        </h2>
        {kits.length === 0 ? (
          <p class="empty">This build lists no kits yet.</p>
        ) : (
          <div class="home__kits">
            {kits.map((kit, index) => (
              <KitPanel
                key={kit.id}
                name={kit.name ?? kit.id}
                line={kit.line ?? ""}
                tone={kitTone(index)}
                shown={4}
                tasks={(kit.tasks ?? []).map((task) => ({
                  title: task.title ?? task.name,
                  shape: task.shape ?? "",
                  uses: task.uses,
                  href: hrefOf({ name: "kit", kit: kit.id, task: task.name }),
                }))}
              />
            ))}
          </div>
        )}
      </section>

      <section class="home__section" aria-labelledby="pictures-title">
        <h2 id="pictures-title" class="section-title">
          Pictures
        </h2>
        <p class="empty">No pictures yet. A picture you give a task is kept in the store and shows here.</p>
      </section>
    </main>
  );
}
