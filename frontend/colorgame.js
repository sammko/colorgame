import { DateTime } from 'luxon';

//const rounds = [10, 8, 8, 7.5, 7, 6.5, 6, 5.5, 5, 4.5, 4, 3.5, 3];
const rounds = [0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2];

const query = new URLSearchParams(window.location.search);
const gameStartStr = query.get("start");

if (gameStartStr === null) {
  alert("start parameter missing");
  throw new Error("start parameter missing")
}

const gameStart = DateTime.fromISO(gameStartStr);

if (!gameStart.isValid) {
  console.log(gameStart)
  alert("Invalid start parameter value");
  throw new Error("Invalid start parameter value")
}

let currentRound = 0;
const roundEnds = [];

{
  const now = DateTime.now();
  let sum = 0;
  for (let i = 0; i < rounds.length; i++) {
    const length = rounds[i];
    sum += length;
    let t = gameStart.plus({ minutes: sum });
    roundEnds.push(t);
    if (now > t) currentRound = i + 1;
  }
}

const assignments =
{
  0: ["", "", ""],
  1: ["Farby dúhy", "červená, žltá, zelená, modrá", "1.jpg"],
  2: ["Farby CMYK", "žltá, tyrkysová, magenta, čierna", ""],
  3: ["Farby listov na jeseň", "zelená, červená, žltá, magenta", "3.jpg"],
  4: ["Farby tričiek vedúcich na tejto hre", "TODO", ""],
  5: ["Farby RGB", "červená, zelená, modrá", "5.jpg"],
  6: ["Šachovnica", "čierna, biela", "6.jpg"],
  7: ["Také farby ako má Viki na ponožkách " +
    "a Hovorca na tričku", "tyrkysová, magenta", ""],
  8: ["Každý člen družinky má inú farbu", "", ""],
  9: ["Farby slovenskej vlajky", "biela, modrá, červená", "9.jpg"],
  10: ["Farby Linux loga", "čierna, biela, žltá", "10.jpg"],
  11: ["Farby vodnej hladiny", "modrá, tyrkysová, zelená", "11.jpeg"],
  12: ["Čo najunikátnejšia farba na sústredku", "", ""]
}

const $e = (elName, elClass, elParent) => {
  let e = document.createElement(elName);
  if (elClass) e.classList.add(elClass);
  if (elParent) elParent.appendChild(e);
  return e;
}

function clear_tables() {
  for (let i = 1; i <= 6; i++) {
    let table = document.getElementById("barcode-colors-table" + (i).toString());
    let tbody = table.firstChild
    while (tbody.childElementCount > 1) tbody.removeChild(tbody.lastChild);
  }
}

async function load_state() {
  const response = await fetch("https://colorgame.fly.dev/current", {
    headers: {
      "Authorization": "Bearer " + localStorage.getItem("token")
    }
  });
  if (!response.ok) {
    throw new Error(`Error fetching current state: ${response.status} - ${response.statusText}`)
  } else {
    let data = await response.json();
    clear_tables();
    let empty = true;
    for (const [key, value] of Object.entries(data)) {
      let table = document.getElementById("barcode-colors-table" + (parseInt((key - 1) / 8 + 1)).toString());
      let row = $e('tr', '', table.firstChild);
      let bar = $e('td', 'barcode', row);
      bar.innerHTML = key;
      let wrapper = $e('td', 'wrap', row)
      let color = $e('div', value, wrapper);
      color.classList += ' color_square'
      empty = false;
    }
    return empty;
  }
}

function updateTimer() {
  const currentRoundEnd = roundEnds[currentRound];
  if (DateTime.now() > currentRoundEnd) {
    currentRound++;
    spustiKolo(currentRound);
  } else {
    document.getElementById("timer").innerHTML = currentRoundEnd.diff(DateTime.now()).toFormat("mm:ss")
  }
}

function zahrajNahravky(zoznam) {
  if (zoznam.length == 0)
    return;
  var audio = new Audio('audio/' + zoznam[0] + '.mp3');
  audio.onended = function () {
    zahrajNahravky(zoznam.slice(1, undefined));
  };
  audio.play();
}

function zahlasKolo(x) {
  var finalnyZoznam = ["bell"];
  if (x == 0)
    finalnyZoznam.push("0");
  else {
    let zoznam = [100, 90, 80, 70, 60, 50, 40, 30, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
    zoznam.forEach((a) => { if (x >= a) { finalnyZoznam.push("" + a); x -= a; } });
  }

  finalnyZoznam.push("kolo");
  zahrajNahravky(finalnyZoznam);
}

function spustiKolo(x) {
  if (x >= rounds.length) {
    clearInterval(timeUpdater);
    clearInterval(stateUpdater);
    document.getElementById("round").innerHTML = "Koniec hry";
    return;
  }

  zahlasKolo(x);
  document.getElementById("round").innerHTML = x + ". kolo";
  document.getElementById("as_header").innerHTML = assignments[x][0];
  document.getElementById("as_text").innerHTML = assignments[x][1];
  if (assignments[x][2] != "")
    document.getElementById("as_img").src = "assignments/" + assignments[x][2];
  else
    document.getElementById("as_img").src = "";
}

load_state();

spustiKolo(currentRound);

var timeUpdater = setInterval(updateTimer, 100);
var stateUpdater = setInterval(load_state, 1000);